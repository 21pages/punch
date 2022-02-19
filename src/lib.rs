use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json;
use std::{net::SocketAddr, time::Duration};
use tokio::{
    net::{lookup_host, TcpListener, TcpSocket, TcpStream, ToSocketAddrs},
    time::timeout,
};

//用于hybrid_client/hybrid_server
pub mod hybrid {
    use super::*;

    #[allow(non_camel_case_types)]
    #[derive(Serialize, Deserialize, Debug)]
    pub enum Message {
        register_request(String),     // id_A
        register_response(u8),        // one byte
        punchA2S(String),             // id_B
        punchS2B(SocketAddr),         // A_tcp_addr
        punchB2S(u8),                 // one byte
        punchS2A(Option<SocketAddr>), // B_tcp_addr
        messageAB(String),            // message between AB
    }
    impl Message {
        pub fn encode(&self) -> Vec<u8> {
            serde_json::to_vec(self).unwrap()
        }

        pub fn decode(data: &[u8]) -> Result<Self> {
            Ok(serde_json::from_slice(data)?)
        }
    }
}

//用于tcp/udp
pub mod simple {
    use super::*;

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

pub async fn new_tcp_stream<T1: ToSocketAddrs, T2: ToSocketAddrs>(
    remote_addr: T1,
    local_addr: T2,
    sec_timeout: u64,
) -> Result<TcpStream> {
    for local_addr in lookup_host(&local_addr).await? {
        for remote_addr in lookup_host(&remote_addr).await? {
            let stream = timeout(
                Duration::from_secs(sec_timeout),
                new_tcp_socket(local_addr, true)?.connect(remote_addr),
            )
            .await??;
            stream.set_nodelay(true).ok();
            return Ok(stream);
        }
    }
    bail!("could not resolve to any address");
}

#[allow(clippy::never_loop)]
pub async fn new_tcp_listener<T: ToSocketAddrs>(addr: T, reuse: bool) -> Result<TcpListener> {
    if !reuse {
        Ok(TcpListener::bind(addr).await?)
    } else {
        for addr in lookup_host(&addr).await? {
            let socket = new_tcp_socket(addr, true)?;
            return Ok(socket.listen(32)?);
        }
        bail!("could not resolve to any address");
    }
}

#[macro_export]
macro_rules! allow_err {
    ($e:expr) => {
        if let Err(err) = $e {
            log::warn!(
                "{:?}, {}:{}:{}:{}",
                err,
                module_path!(),
                file!(),
                line!(),
                column!()
            );
        } else {
        }
    };
}
