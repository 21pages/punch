use anyhow::Result;
use log;
use punch::hybrid::*;
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use tokio::{
    self,
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket},
    sync::Mutex,
};

#[tokio::main]
async fn main() -> Result<()> {
    std::env::set_var("RUST_LOG", "info");
    env_logger::init();

    let server_addr = std::env::var("ADDR")
        .unwrap()
        .parse::<SocketAddr>()
        .unwrap();

    let id_map = Arc::new(Mutex::new(HashMap::<String, SocketAddr>::new()));
    let udp_listener = Arc::new(UdpSocket::bind(server_addr).await.unwrap());
    let tcp_listener = TcpListener::bind(server_addr).await.unwrap();
    log::info!("listening on {:?}", tcp_listener.local_addr());

    let handle1 = tokio::spawn(handle_udp(udp_listener.clone(), id_map.clone()));
    let handle2 = tokio::spawn(handle_tcp(
        tcp_listener,
        udp_listener.clone(),
        id_map.clone(),
    ));

    let _ = tokio::join!(handle1, handle2);

    return Ok(());
}

async fn handle_tcp(
    tcp_listener: TcpListener,
    udp_socket: Arc<UdpSocket>,
    id_map: Arc<Mutex<HashMap<String, SocketAddr>>>,
) -> Result<()> {
    let mut buf = vec![0u8; 1024];
    let mut saved_stream_a: Option<TcpStream> = None;
    loop {
        tokio::select! {
            Ok((mut stream, addr)) = tcp_listener.accept() => {
                log::info!("new client from {:?}", addr);
                match stream.read(&mut buf).await {
                    Ok(n) => {
                        if let Ok(msg) = Message::decode(&buf[..n]) {
                            log::info!("tcp recv {:?} from {:?}", msg, addr);
                            match msg {
                                //来自A的打洞请求
                                Message::punchA2S(id_b) => {
                                    let map = id_map.lock().await;
                                    if let Some(b_udp_addr) = map.get(&id_b) {
                                        //B已注册, 向B发访问请求
                                        let punch_s2b = Message::punchS2B(addr.clone()); //a_tcp_addr
                                        match udp_socket.send_to(&punch_s2b.encode(), b_udp_addr).await {
                                            Ok(_) => log::info!("Send A addr {:?} to B:{:?}", addr, b_udp_addr),
                                            Err(e) => log::error!("Failed to Send udp to B: {:?}", e)
                                        }
                                    } else {
                                        //B未注册,立即回复A
                                        let punch_s2a =  Message::punchS2A(None);
                                        match stream.write_all(&punch_s2a.encode()).await {
                                            Ok(_) => log::info!("send A {:? }that B not register", addr),
                                            Err(e) => log::error!("Failed to send tcp to A:{:?}", e)
                                        }
                                    }
                                    saved_stream_a = Some(stream);
                                }
                                //来自B的打洞回复
                                Message::punchB2S(_) => {
                                    if let Some(mut stream_a) = saved_stream_a.take() {
                                        let punch_s2a =  Message::punchS2A(Some(addr.clone())); //b_tcp_addr
                                        match stream_a.write_all(&punch_s2a.encode()).await {
                                            Ok(_) => log::info!("send A {:?} addr of B {:?}", &stream_a.peer_addr(), stream.peer_addr()),
                                            Err(e) => log::error!("Failed to send tcp to A:{:?}", e)
                                        }
                                    } else {
                                        log::error!("saved_stream_a is None");
                                    }
                                }
                                _ => {
                                    log::warn!("tcp recv msg {:?}", msg);
                                }
                            }
                        } else {
                            log::error!("tcp msg decode failed");
                        }
                    }
                    Err(e) => log::error!("Read failed: {:?}", e)
                }
            }
        }
    }
}

async fn handle_udp(
    listener: Arc<UdpSocket>,
    id_map: Arc<Mutex<HashMap<String, SocketAddr>>>,
) -> Result<()> {
    let mut buf = vec![0u8; 1024];
    loop {
        tokio::select! {
            Ok((len, addr)) = listener.recv_from(&mut buf) => {
                log::debug!("udp new msg from {:?}", addr);
                if let Ok(msg) = Message::decode(&buf[..len]) {
                    match msg {
                        Message::register_request(reg) => {
                            //更新udp 地址
                            log::debug!("{:?} id {} register", addr, reg);
                            id_map.lock().await.insert(reg.clone(), addr.clone());

                            //注册确认
                            let rsp = Message::register_response(0);
                            if let Err(e) = listener.send_to(&rsp.encode(), addr).await {
                                log::error!("Send rsp to {:?} failed. {:?}", addr, e);
                            } else {
                                log::debug!("send {:?} to addr {:?}", rsp, addr);
                            }
                        }
                        _ => {
                            log::warn!("udp recv {:?}", msg);
                        }
                    }
                } else {
                    log::error!("udp msg decode failed");
                }
            }
        }
    }
}

/*
ADDR="0.0.0.0:12345" cargo run --bin hybrid_server

[2022-02-20T08:11:13Z INFO  hybrid_server] new client from 27.216.129.86:3854
[2022-02-20T08:11:24Z INFO  hybrid_server] tcp recv punchA2S("B") from 27.216.129.86:3854
[2022-02-20T08:11:24Z INFO  hybrid_server] Send A addr 27.216.129.86:3854 to B:101.34.84.73:49612
[2022-02-20T08:11:24Z INFO  hybrid_server] new client from 101.34.84.73:33714
[2022-02-20T08:11:27Z INFO  hybrid_server] tcp recv punchB2S(0) from 101.34.84.73:33714
[2022-02-20T08:11:27Z INFO  hybrid_server] send A Ok(27.216.129.86:3854) addr of B Ok(101.34.84.73:33714)
*/
