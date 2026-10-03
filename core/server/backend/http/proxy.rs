//! Only the explicitly configured, local Cloudflare Tunnel may supply client IPs.
use super::{Error, Result};
use axum::http::HeaderMap;
use std::net::IpAddr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrustedProxy {
    None,
    Cloudflare,
}
impl TrustedProxy {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "none" => Ok(Self::None),
            "cloudflare" => Ok(Self::Cloudflare),
            _ => Err(Error::new("invalid_trusted_proxy", 400)),
        }
    }
    pub fn client_ip(self, peer: IpAddr, headers: &HeaderMap) -> Result<String> {
        let peer = peer.to_canonical();
        if self != Self::Cloudflare || !peer.is_loopback() {
            return Ok(peer.to_string());
        }
        let mut values = headers.get_all("cf-connecting-ip").iter();
        // Direct local probes have no proxy header and keep their own allowance.
        let Some(value) = values.next() else {
            return Ok(peer.to_string());
        };
        let address = value.to_str().ok().and_then(|v| v.parse::<IpAddr>().ok());
        match (address, values.next()) {
            (Some(address), None) => Ok(address.to_canonical().to_string()),
            _ => Err(Error::new("invalid_client_address", 400)),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/backend/http/proxy.rs"]
mod tests;
