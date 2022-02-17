use anyhow::Result;
use clap::Parser;
use log;
use punch::*;
use std::net::SocketAddr;
use tokio::{
    self,
    net::UdpSocket,
    time::{interval, sleep, timeout, Duration},
};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Server addr
    #[clap(short, long)]
    server: String,

    /// id
    #[clap(short, long)]
    id: String,

    /// peer id
    #[clap(short, long)]
    peer_id: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    std::env::set_var("RUST_LOG", "info");
    env_logger::init();
    let args = Args::parse();
    let mut buf = vec![0u8; 1024];

    let socket = UdpSocket::bind("0.0.0.0:0").await?;

    // step 1: connect server
    loop {
        let addr: SocketAddr = args.server.parse().expect("bad server addr");
        match timeout(Duration::from_secs(3), socket.connect(addr)).await {
            Ok(_) => {
                log::info!("connect to {} ok!", addr);
                break;
            }
            Err(e) => {
                log::warn!("connect to {} time out :{:?}", addr, e);
            }
        }
    }

    // step 2: register and get peer addr
    #[allow(unused_assignments)]
    let mut peer_addr: Option<SocketAddr> = None;
    loop {
        sleep(Duration::from_secs(1)).await;
        let msg = Register {
            id: args.id.clone(),
            peer_id: args.peer_id.clone(),
        };
        match socket.send(&msg.encode()).await {
            Ok(_) => log::info!("send register ok"),
            Err(e) => {
                log::error!("send register falied. {:?}", e);
                continue;
            }
        }

        match timeout(Duration::from_secs(10), socket.recv(&mut buf)).await {
            Ok(Ok(size)) => {
                if let Ok(peer) = Peer::decode(&buf[..size]) {
                    if let Some(addr) = peer.peer_addr {
                        log::info!("get peer addr {:?}", addr);
                        peer_addr = Some(addr);
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

    // step 3 connect to peer
    let peer_addr = peer_addr.unwrap();
    loop {
        match timeout(Duration::from_secs(3), socket.connect(&peer_addr)).await {
            Ok(_) => {
                log::info!("connect to peer {:?} success.", peer_addr);
                break;
            }
            Err(e) => {
                log::warn!("Failed to connect to peer {:?} {:?}", peer_addr, e);
            }
        }
    }

    // step 4 send & recv message

    let mut timer = interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            _ = timer.tick() => {
                let msg = format!("hello {} I'm {}", args.peer_id, args.id);
                match socket.send(msg.as_bytes()).await {
                    Ok(_) => log::info!("send ok"),
                    Err(e) => log::warn!("send failed:{:?}", e),
                }
            }
            n = socket.recv(&mut buf) => {
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
udp_client.exe --server "101.34.84.73:12345" --id "1" --peer-id "2"
client2:
udp_client.exe --server "101.34.84.73:12345" --id "2" --peer-id "1"

log:

client 1:

connect to 101.34.84.73:12345 ok!
send register ok
peer is not registered yet
...
get peer addr 27.216.129.86:60573
connect to peer 27.216.129.86:60573 success.
send ok
recv hello 1 I'm 2 from 2
send ok
recv hello 1 I'm 2 from 2

client 2:

connect to 101.34.84.73:12345 ok!
send register ok
get peer addr 112.224.157.91:58422
connect to peer 112.224.157.91:58422 success.
send ok
recv hello 2 I'm 1 from 1
send ok
recv hello 2 I'm 1 from 1
*/
