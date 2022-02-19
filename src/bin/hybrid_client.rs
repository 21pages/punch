use anyhow::{bail, Result};
use clap::Parser;
use log;
use punch::{hybrid::Message, *};
use std::{net::SocketAddr, sync::Arc};
use tokio::{
    self,
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpStream, UdpSocket},
    sync::{mpsc, Notify},
    time::{interval, timeout, Duration},
};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Server addr
    #[clap(short, long)]
    server: String,

    /// ID
    #[clap(short, long)]
    id: String,

    /// Point out Peer id when connect actively
    #[clap(short, long)]
    peer_id: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    std::env::set_var("RUST_LOG", "info");
    env_logger::init();

    let args = Args::parse();
    let server_addr: SocketAddr = args.server.parse().expect("bad server addr");
    let id = args.id;
    let peer_id = args.peer_id;

    let (sender, receiver) = mpsc::channel::<SocketAddr>(1);
    let notify_register = Arc::new(Notify::new());

    let handle_udp = tokio::spawn(udp_task(
        server_addr.clone(),
        id.clone(),
        sender,
        notify_register.clone(),
    ));

    let handle_tcp = if peer_id.is_some() {
        tokio::spawn(tcp_task_a(
            server_addr,
            peer_id.unwrap(),
            notify_register.clone(),
        ))
    } else {
        tokio::spawn(tcp_task_b(server_addr, notify_register.clone(), receiver))
    };

    let _ = tokio::join!(handle_udp, handle_tcp);

    Ok(())
}

//主动连接的客户端A
async fn tcp_task_a(
    server_addr: SocketAddr,
    peer_id: String,
    notify_register: Arc<Notify>,
) -> Result<()> {
    log::info!("tcp task a start");
    //step 1:  等待udp注册成功
    notify_register.notified().await;

    //step 2: 连接服务器
    let mut stream = connect_server(server_addr).await?;

    //step 3: 等待命令行敲入打洞命令
    let handle_stdin = tokio::spawn(async move {
        log::info!("Enter any words to start punch...");
        let mut start_punch_command = String::new();
        let _ = std::io::stdin().read_line(&mut start_punch_command).is_ok();
    });
    let _ = tokio::join!(handle_stdin);
    log::info!("get command and start punch hole!");

    //step 4: 主动打洞, 请求B地址
    let punch_a2s = Message::punchA2S(peer_id.clone());
    match stream.write_all(&punch_a2s.encode()).await {
        Ok(_) => log::info!("send punch request to server"),
        Err(e) => {
            log::error!("tcp send failed {:?}", e);
            return Err(anyhow::anyhow!("{:?}", e));
        }
    }

    //step 5: 等待打洞成功
    let mut buf = vec![0u8; 1024];
    if let Ok(Ok(n)) = timeout(Duration::from_secs(10), stream.read(&mut buf)).await {
        if let Ok(message) = Message::decode(&buf[..n]) {
            log::info!("tcp recv {:?}", message);
            if let Message::punchS2A(tcp_addr_b) = message {
                if let Some(tcp_addr_b) = tcp_addr_b {
                    let local_addr = stream.local_addr().unwrap();
                    drop(stream);
                    loop {
                        let _ = tokio::time::sleep(Duration::from_secs(1));
                        log::info!(
                            "try new_tcp_stream with local: {:?}, remote:{:?}",
                            local_addr,
                            tcp_addr_b
                        );
                        match new_tcp_stream(tcp_addr_b.clone(), local_addr.clone(), 5).await {
                            Ok(stream) => {
                                let _ = chat(stream).await;
                                continue;
                            }
                            Err(e) => log::error!("Failed new_tcp_stream {:?}", e),
                        }
                    }
                } else {
                    log::warn!("punch failed. B not register");
                }
            }
        }
    } else {
        log::warn!("punch failed. timeout")
    }

    Ok(())
}

//被动连接的客户端B
async fn tcp_task_b(
    server_addr: SocketAddr,
    notify_register: Arc<Notify>,
    mut receiver: mpsc::Receiver<SocketAddr>,
) -> Result<()> {
    log::info!("tcp task b start.waiting notify...");
    //step 1:  等待udp注册成功
    notify_register.notified().await;

    //step 2: 等待udp传来A的地址
    let tcp_addr_a = receiver.recv().await.unwrap();

    //step 3: 连接服务器, 获取本地地址
    let mut stream = connect_server(server_addr).await?;
    let local_addr = stream.local_addr().unwrap();
    log::info!("my local addr:{:?}", local_addr);

    // step 4: 向A发一个任意消息
    if let Ok(mut test_stream) = new_tcp_stream(tcp_addr_a.clone(), local_addr.clone(), 3).await {
        if test_stream.write_all(b" ").await.is_ok() {
            log::info!("send one byte ok");
        }
    }

    //step 5: 回复服务器, 使其获取自己的地址
    let punch_b2s = Message::punchB2S(0);
    match stream.write_all(&punch_b2s.encode()).await {
        Ok(_) => log::info!("send punch response to server"),
        Err(e) => {
            log::error!("tcp send failed {:?}", e);
            return Err(anyhow::anyhow!("{:?}", e));
        }
    }
    drop(stream);

    //step 6: 监听端口, 等待连接
    log::info!("try listen at {:?}", local_addr);
    let listener = new_tcp_listener(local_addr, true).await?;
    log::info!("listen at {:?} ok", local_addr);
    // let listener = TcpListener::bind(local_addr).await?;
    if let Ok((stream, addr)) = listener.accept().await {
        log::info!("accept connection {:?}", addr);
        if addr != tcp_addr_a {
            log::error!("different addr:{:?}, {:?}", addr, tcp_addr_a);
        }
        chat(stream).await?;
    }

    Ok(())
}

async fn connect_server(server_addr: SocketAddr) -> Result<TcpStream> {
    loop {
        match timeout(
            Duration::from_secs(3),
            new_tcp_stream(server_addr, "0.0.0.0:0", 3),
        )
        .await
        {
            Ok(Ok(stream)) => {
                log::info!("tcp connect to server {} ok!", server_addr);
                return Ok(stream);
            }
            _ => {
                log::warn!("tcp connect to server {} time out", server_addr);
                continue;
            }
        }
    }
}

async fn chat(mut stream: TcpStream) -> Result<()> {
    log::info!(
        "begin chat.local:{:?} peer:{:?}",
        stream.local_addr(),
        stream.peer_addr()
    );
    let mut buf = vec![0u8; 1024];
    let mut timer = interval(Duration::from_secs(1));

    loop {
        tokio::select! {
            Ok(n) = stream.read(&mut buf) => {
                log::info!("recv {} bytes", n);
                if n == 0 {
                    drop(stream);
                    bail!("recv 0 bytes");
                }
            }
            _ = timer.tick() => {
                match stream.write_all(&Message::messageAB("hello".to_owned()).encode()).await {
                    Ok(_) =>log::info!("send chat"),
                    Err(e) => {
                        log::error!("send failed: {:?}", e);
                        drop(stream);
                        bail!("send failed");
                    }
                }
            }

        }
    }
}

async fn udp_task(
    server_addr: SocketAddr,
    id: String,
    sender: mpsc::Sender<SocketAddr>,
    notify_register: Arc<Notify>,
) -> Result<()> {
    log::info!("start udp task...");
    let socket = UdpSocket::bind("0.0.0.0:0").await.unwrap();
    let mut buf = vec![0u8; 1024];

    // step 1: connect server
    loop {
        match timeout(Duration::from_secs(3), socket.connect(&server_addr)).await {
            Ok(_) => {
                log::info!("udp connect to {:?} ok!", server_addr);
                break;
            }
            Err(e) => {
                log::warn!("udp connect to {:?} time out :{:?}", server_addr, e);
            }
        }
    }

    //step 2: regiter repeatly and waiting punch request
    let mut timer = interval(Duration::from_secs(3));

    loop {
        tokio::select! {
            _ = timer.tick() => {
                let register_request = Message::register_request(id.clone());
                match socket.send(&register_request.encode()).await {
                    Ok(_) => log::debug!("register ok"),
                    Err(e) => log::error!("failed to register {:?}", e)
                }
            }
            Ok(n) = socket.recv(&mut buf) => {
                if let Ok(message) = Message::decode(&buf[..n]) {
                    match message {
                        Message::register_response(_) => {
                            log::debug!("register response");
                            notify_register.notify_one();
                        }
                        Message::punchS2B(tcp_addr_a) => {
                            log::info!("B recv tcp_addr_a: {:?}", tcp_addr_a);
                            sender.send(tcp_addr_a).await.ok();
                        }
                        _ => log::warn!("other udp message {:?}", message),
                    }
                }
            }
        }
    }
}

/*
client A:
./hybrid_client.exe --server "101.34.84.73:12345" --id A --peer-id B

client B:
./hybrid_client --server "101.34.84.73:12345" --id B

log:

client A:
$ ./hybrid_client.exe --server "120.26.127.138:12345" --id A --peer-id B
[2022-02-20T08:11:13Z INFO  hybrid_client] start udp task...
[2022-02-20T08:11:13Z INFO  hybrid_client] tcp task a start
[2022-02-20T08:11:13Z INFO  hybrid_client] udp connect to 120.26.127.138:12345 ok!
[2022-02-20T08:11:13Z INFO  hybrid_client] tcp connect to server 120.26.127.138:12345 ok!
[2022-02-20T08:11:13Z INFO  hybrid_client] Enter any words to start punch...

[2022-02-20T08:11:24Z INFO  hybrid_client] get command and start punch hole!
[2022-02-20T08:11:24Z INFO  hybrid_client] send punch request to server
[2022-02-20T08:11:27Z INFO  hybrid_client] tcp recv punchS2A(Some(101.34.84.73:33714))
[2022-02-20T08:11:27Z INFO  hybrid_client] try new_tcp_stream with local: 192.168.1.6:3854, remote:101.34.84.73:33714
[2022-02-20T08:11:27Z INFO  hybrid_client] begin chat.local:Ok(192.168.1.6:3854) peer:Ok(101.34.84.73:33714)
[2022-02-20T08:11:27Z INFO  hybrid_client] send chat
[2022-02-20T08:11:27Z INFO  hybrid_client] recv 21 bytes
[2022-02-20T08:11:28Z INFO  hybrid_client] send chat
[2022-02-20T08:11:28Z INFO  hybrid_client] recv 21 bytes
[2022-02-20T08:11:29Z INFO  hybrid_client] send chat
[2022-02-20T08:11:29Z INFO  hybrid_client] recv 21 bytes

client B:
[root@VM-0-6-centos debug]# ./hybrid_client --server "120.26.127.138:12345" --id B
[2022-02-20T08:11:20Z INFO  hybrid_client] start udp task...
[2022-02-20T08:11:20Z INFO  hybrid_client] udp connect to 120.26.127.138:12345 ok!
[2022-02-20T08:11:20Z INFO  hybrid_client] tcp task b start.waiting notify...
[2022-02-20T08:11:24Z INFO  hybrid_client] B recv tcp_addr_a: 27.216.129.86:3854
[2022-02-20T08:11:24Z INFO  hybrid_client] tcp connect to server 120.26.127.138:12345 ok!
[2022-02-20T08:11:24Z INFO  hybrid_client] my local addr:172.17.0.6:33714
[2022-02-20T08:11:27Z INFO  hybrid_client] send punch response to server
[2022-02-20T08:11:27Z INFO  hybrid_client] try listen at 172.17.0.6:33714
[2022-02-20T08:11:27Z INFO  hybrid_client] listen at 172.17.0.6:33714 ok
[2022-02-20T08:11:27Z INFO  hybrid_client] accept connection 27.216.129.86:3854
[2022-02-20T08:11:27Z INFO  hybrid_client] begin chat.local:Ok(172.17.0.6:33714) peer:Ok(27.216.129.86:3854)
[2022-02-20T08:11:27Z INFO  hybrid_client] send chat
[2022-02-20T08:11:27Z INFO  hybrid_client] recv 21 bytes
[2022-02-20T08:11:28Z INFO  hybrid_client] send chat
[2022-02-20T08:11:28Z INFO  hybrid_client] recv 21 bytes
[2022-02-20T08:11:29Z INFO  hybrid_client] send chat
[2022-02-20T08:11:29Z INFO  hybrid_client] recv 21 bytes

*/
