use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json;
use std::net::SocketAddr;
use tokio::net::TcpSocket;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Register {
    pub id: String,
    pub peer_id: String,
}

impl Register {
    pub fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap()
    }

    pub fn decode(data: &[u8]) -> Result<Self> {
        Ok(serde_json::from_slice(data)?)
    }
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Peer {
    pub peer_addr: Option<SocketAddr>,
}

impl Peer {
    pub fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap()
    }

    pub fn decode(data: &[u8]) -> Result<Self> {
        Ok(serde_json::from_slice(data)?)
    }
}

pub fn new_tcp_socket(
    addr: std::net::SocketAddr,
    reuse: bool,
) -> Result<TcpSocket, std::io::Error> {
    let socket = match addr {
        std::net::SocketAddr::V4(..) => TcpSocket::new_v4()?,
        std::net::SocketAddr::V6(..) => TcpSocket::new_v6()?,
    };
    if reuse {
        // windows has no reuse_port, but it's reuse_address
        // almost equals to unix's reuse_port + reuse_address,
        // though may introduce nondeterministic behavior
        #[cfg(unix)]
        socket.set_reuseport(true)?;
        socket.set_reuseaddr(true)?;
    }
    socket.bind(addr)?;
    Ok(socket)
}
