use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json;
use std::net::SocketAddr;

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
