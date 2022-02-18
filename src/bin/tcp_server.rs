use anyhow::Result;
use log;
use punch::simple::*;
use std::{collections::HashMap, net::SocketAddr};
use tokio::{
    self,
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

#[tokio::main]
async fn main() -> Result<()> {
    std::env::set_var("RUST_LOG", "info");
    env_logger::init();
    let listener = TcpListener::bind(
        std::env::var("ADDR")
            .unwrap()
            .parse::<SocketAddr>()
            .unwrap(),
    )
    .await?;
    let mut buf = vec![0u8; 1024];
    let mut id_map = HashMap::<String, SocketAddr>::new();
    log::info!("listening on {:?}", listener.local_addr());

    loop {
        tokio::select! {
            Ok((mut stream, addr)) = listener.accept() => {
                log::info!("new client from {:?}", addr);
                if stream.readable().await.is_ok() {
                    match stream.read(&mut buf).await {
                        Ok(n) => {
                            if let Ok(reg) = Register::decode(&buf[..n]) {
                                log::info!("{:?} id {} want {}", addr, reg.id, reg.peer_id);
                                id_map.insert(reg.id.clone(), addr.clone());

                                let mut rsp = Peer::default(); //默认peer未注册
                                if id_map.contains_key(&reg.peer_id) {
                                    rsp.peer_addr = Some(id_map.get(&reg.peer_id).unwrap().clone()) //peer地址
                                }
                                if let Err(e) = stream.write_all(&rsp.encode()).await {
                                    log::error!("Send rsp to {:?} failed. {:?}", addr, e);
                                } else {
                                    log::info!("send {:?} to addr {:?}", rsp, addr);
                                }
                            } else {
                                log::error!("decode failed");
                            }
                        }
                        Err(e) => log::error!("Read failed: {:?}", e)
                    }
                }
            }
        }
    }
}

/*
ADDR="0.0.0.0:12345" cargo run --bin tcp_server
*/
