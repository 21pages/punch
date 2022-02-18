use anyhow::Result;
use clap::Parser;
use log;
use punch::{new_tcp_socket, simple::*};
use std::net::SocketAddr;
use tokio::{
    self,
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::{interval, sleep, timeout, Duration},
};

/*
用tcp 去注册/心跳
    1. 一直复用端口会出现连接失败的情况
    2. 不复用端口, 会出现两个A,B一前一后,第二个连上服务器的获取的是过时的地址,
       因为第一个再连的时候地址变了, 之后监听的端口也变了, 所以做listener的不能一直连

*/

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Server addr
    #[clap(short, long)]
    server: String,

    /// ID
    #[clap(short, long)]
    id: String,

    /// Peer id
    #[clap(short, long)]
    peer_id: String,

    /// As listener
    #[clap(long)]
    listener: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    std::env::set_var("RUST_LOG", "info");
    env_logger::init();
    let args = Args::parse();
    let mut buf = vec![0u8; 1024];

    // step 1: connect server && register && get peer addr and local addr

    let server_addr: SocketAddr = args.server.parse().expect("bad server addr");
    #[allow(unused_assignments)]
    let mut peer_addr: Option<SocketAddr> = None;
    #[allow(unused_assignments)]
    let mut local_addr: Option<SocketAddr> = None;

    loop {
        sleep(Duration::from_secs(1)).await;

        // connect server
        #[allow(unused_assignments)]
        let mut stream: Option<TcpStream> = None;
        match timeout(Duration::from_secs(3), TcpStream::connect(server_addr)).await {
            Ok(Ok(s)) => {
                stream = Some(s);
                log::info!("connect to {} ok!", server_addr);
            }
            _ => {
                log::warn!("connect to {} time out", server_addr);
                continue;
            }
        }
        let mut stream = stream.unwrap();

        // register
        let msg = Register {
            id: args.id.clone(),
            peer_id: args.peer_id.clone(),
        };
        match stream.write_all(&msg.encode()).await {
            Ok(_) => log::info!("send register ok"),
            Err(e) => {
                log::error!("send register falied. {:?}", e);
                continue;
            }
        }

        // get peer addr and local addr
        match timeout(Duration::from_secs(10), stream.read(&mut buf)).await {
            Ok(Ok(size)) => {
                if let Ok(peer) = Peer::decode(&buf[..size]) {
                    if let Some(addr) = peer.peer_addr {
                        log::info!("get peer addr {:?}", addr);
                        peer_addr = Some(addr);
                        local_addr = Some(stream.local_addr().unwrap());
                        break;
                    } else {
                        log::info!("peer is not registered yet");
                    }
                }
            }
            _ => {
                log::warn!("wait register response timeout");
                continue;
            }
        }
    }
    let peer_addr = peer_addr.unwrap();
    let local_addr = local_addr.unwrap();

    // step 2 get stream
    #[allow(unused_assignments)]
    let mut stream2: Option<TcpStream> = None; //AB对话的stream
    if args.listener {
        //监听之前的端口
        let listener = TcpListener::bind(local_addr).await?;
        log::info!("A listening at {:?}", local_addr);
        match listener.accept().await {
            Ok((stream, addr)) => {
                log::info!("accept client from {:?}", addr);
                if addr == peer_addr {
                    stream2 = Some(stream);
                } else {
                    log::error!("expect {:?}, but accept {:?}", peer_addr, addr)
                }
            }
            Err(e) => log::error!("Failed accept: {:?}", e),
        }
    } else {
        loop {
            //用之前的端口去连接
            let socket = new_tcp_socket(local_addr, true)?;
            match timeout(Duration::from_secs(3), socket.connect(peer_addr.clone())).await {
                Ok(Ok(stream)) => {
                    log::info!("connect to peer {:?} success.", peer_addr);
                    stream2 = Some(stream);
                    break;
                }
                _ => {
                    log::warn!("Failed to connect to peer {:?}", peer_addr);
                }
            }
        }
    }
    let mut stream = stream2.unwrap();

    // step 4 send & recv message

    let mut timer = interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            _ = timer.tick() => {
                let msg = format!("hello {} I'm {}", args.peer_id, args.id);
                match stream.write_all(msg.as_bytes()).await {
                    Ok(_) => log::info!("send ok"),
                    Err(e) => log::warn!("send failed:{:?}", e),
                }
            }
            n = stream.read(&mut buf) => {
                match n {
                    Ok(n) => {
                        let msg = String::from_utf8_lossy(&buf[..n]);
                        log::info!("recv {} from {}", msg, args.peer_id);
                    }
                    Err(e) => log::warn!("recv failed:{:?}", e),
                }
            }
        }
    }
}

/*
client1:
tcp_client.exe --server "101.34.84.73:12345" --id "1" --peer-id "2"
client2:
tcp_client.exe --server "101.34.84.73:12345" --id "2" --peer-id "1"

log:
*/
