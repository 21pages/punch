use anyhow::Result;
use log;
use punch::*;
use std::{collections::HashMap, net::SocketAddr};
use tokio::{self, net::UdpSocket};

#[tokio::main]
async fn main() -> Result<()> {
    std::env::set_var("RUST_LOG", "info");
    env_logger::init();
    let socket = UdpSocket::bind(
        std::env::var("ADDR")
            .unwrap()
            .parse::<SocketAddr>()
            .unwrap(),
    )
    .await?;
    let mut buf = vec![0u8; 1024];
    let mut id_map = HashMap::<String, SocketAddr>::new();
    log::info!("listening on {:?}", socket.local_addr());

    loop {
        tokio::select! {
            Ok((len, addr)) = socket.recv_from(&mut buf) => {
                log::info!("new msg from {:?}", addr);
                if let Ok(req) = Register::decode(&buf[..len]) {
                    log::info!("{:?} id {} want {}", addr, req.id, req.peer_id);
                    id_map.insert(req.id.clone(), addr.clone());

                    let mut rsp = Peer::default(); //默认peer未注册
                    if id_map.contains_key(&req.peer_id) {
                        rsp.peer_addr = Some(id_map.get(&req.peer_id).unwrap().clone()) //peer地址
                    }
                    if let Err(e) = socket.send_to(&rsp.encode(), addr).await {
                        log::error!("Send rsp to {:?} failed. {:?}", addr, e);
                    } else {
                        log::info!("send {:?} to addr {:?}", rsp, addr);
                    }
                }
            }
        }
    }
}

/*
ADDR="0.0.0.0:12345" cargo run --bin udp_server

log:
send Peer { peer_addr: None } to addr 112.224.157.91:58422
new msg from 112.224.157.91:58422
112.224.157.91:58422 id 1 want 2
send Peer { peer_addr: None } to addr 112.224.157.91:58422
new msg from 27.216.129.86:60573
27.216.129.86:60573 id 2 want 1
send Peer { peer_addr: Some(112.224.157.91:58422) } to addr 27.216.129.86:60573
new msg from 112.224.157.91:58422
112.224.157.91:58422 id 1 want 2
send Peer { peer_addr: Some(27.216.129.86:60573) } to addr 112.224.157.91:58422

*/
