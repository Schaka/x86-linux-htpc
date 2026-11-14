// SPDX-License-Identifier: AGPL-3.0-only
//! SSDP, the discovery of UPnP. The renderer answers searches on the
//! multicast group 239.255.255.250:1900 and announces itself there at the
//! start, every few minutes and, with a goodbye, at the end.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket as StdUdpSocket};
use std::sync::Arc;
use std::time::Duration;

use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;

use super::Dlna;
use super::description::{DEVICE_TYPE, SERVICES};

const GROUP: Ipv4Addr = Ipv4Addr::new(239, 255, 255, 250);
const PORT: u16 = 1900;
const MAX_AGE: u64 = 1800;
const ANNOUNCE_INTERVAL: Duration = Duration::from_secs(600);

pub struct Ssdp {
    dlna: Arc<Dlna>,
    http_port: u16,
    socket: UdpSocket,
    /// A new value at each start tells the control points that the
    /// renderer lost its event subscriptions, so they subscribe again.
    boot_id: u64,
}

impl Ssdp {
    pub fn bind(dlna: Arc<Dlna>, http_port: u16) -> std::io::Result<Ssdp> {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        socket.set_reuse_address(true)?;
        // Other UPnP programs on the same machine listen on the same port.
        socket.set_reuse_port(true)?;
        socket.bind(&SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, PORT)).into())?;
        socket.join_multicast_v4(&GROUP, &Ipv4Addr::UNSPECIFIED)?;
        socket.set_multicast_ttl_v4(2)?;
        socket.set_nonblocking(true)?;
        let socket = UdpSocket::from_std(socket.into())?;
        let boot_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() % (1 << 31))
            .unwrap_or(1);
        Ok(Ssdp {
            dlna,
            http_port,
            socket,
            boot_id,
        })
    }

    /// The notification types: the root device, the UDN, the device type
    /// and the service types, each with its unique service name (USN)
    fn targets(&self) -> Vec<(String, String)> {
        let udn = &self.dlna.udn;
        let mut targets = vec![
            (
                "upnp:rootdevice".to_string(),
                format!("{udn}::upnp:rootdevice"),
            ),
            (udn.clone(), udn.clone()),
            (DEVICE_TYPE.to_string(), format!("{udn}::{DEVICE_TYPE}")),
        ];
        targets.extend(
            SERVICES
                .iter()
                .map(|(_, kind)| (kind.to_string(), format!("{udn}::{kind}"))),
        );
        targets
    }

    /// The address of the description, with the local address that the
    /// peer reaches.
    fn location(&self, peer: SocketAddr) -> Option<String> {
        let probe = StdUdpSocket::bind("0.0.0.0:0").ok()?;
        probe.connect(peer).ok()?;
        let IpAddr::V4(local) = probe.local_addr().ok()?.ip() else {
            return None;
        };
        Some(format!("http://{local}:{}/dlna/device.xml", self.http_port))
    }

    fn server_header() -> String {
        format!("Linux UPnP/1.0 htpc-control/{}", env!("CARGO_PKG_VERSION"))
    }

    pub async fn run(self: Arc<Self>) {
        let announcer = self.clone();
        tokio::spawn(async move {
            loop {
                announcer.announce("ssdp:alive").await;
                tokio::time::sleep(ANNOUNCE_INTERVAL).await;
            }
        });
        let mut buffer = [0u8; 2048];
        loop {
            let Ok((length, peer)) = self.socket.recv_from(&mut buffer).await else {
                continue;
            };
            let text = String::from_utf8_lossy(&buffer[..length]).to_string();
            if let Some((search_target, delay)) = parse_search(&text) {
                let me = self.clone();
                tokio::spawn(async move { me.answer(peer, &search_target, delay).await });
            }
        }
    }

    async fn answer(&self, peer: SocketAddr, search_target: &str, max_delay: u64) {
        let Some(location) = self.location(peer) else {
            return;
        };
        // A random delay up to MX spreads the answers of all devices.
        let millis = (std::process::id() as u64 * 7919 + peer.port() as u64)
            % (max_delay.clamp(1, 3) * 1000);
        tokio::time::sleep(Duration::from_millis(millis.min(1000))).await;
        for (target, usn) in self.targets() {
            if search_target != "ssdp:all" && search_target != target {
                continue;
            }
            let message = format!(
                "HTTP/1.1 200 OK\r\nCACHE-CONTROL: max-age={MAX_AGE}\r\nEXT:\r\nLOCATION: {location}\r\n\
                 SERVER: {}\r\nST: {target}\r\nUSN: {usn}\r\nBOOTID.UPNP.ORG: {}\r\nCONFIGID.UPNP.ORG: 1\r\n\r\n",
                Self::server_header(),
                self.boot_id
            );
            let _ = self.socket.send_to(message.as_bytes(), peer).await;
        }
    }

    /// Sends ssdp:alive or ssdp:byebye for all targets.
    pub async fn announce(&self, kind: &str) {
        let group = SocketAddr::V4(SocketAddrV4::new(GROUP, PORT));
        let Some(location) = self.location(group) else {
            tracing::debug!("SSDP: no network for the announcement");
            return;
        };
        for (target, usn) in self.targets() {
            let mut message = format!(
                "NOTIFY * HTTP/1.1\r\nHOST: {GROUP}:{PORT}\r\nNT: {target}\r\nNTS: {kind}\r\nUSN: {usn}\r\n"
            );
            if kind == "ssdp:alive" {
                message.push_str(&format!(
                    "CACHE-CONTROL: max-age={MAX_AGE}\r\nLOCATION: {location}\r\nSERVER: {}\r\n",
                    Self::server_header()
                ));
            }
            message.push_str(&format!(
                "BOOTID.UPNP.ORG: {}\r\nCONFIGID.UPNP.ORG: 1\r\n\r\n",
                self.boot_id
            ));
            let _ = self.socket.send_to(message.as_bytes(), group).await;
        }
    }
}

/// The search target and MX of an M-SEARCH request
fn parse_search(text: &str) -> Option<(String, u64)> {
    let mut lines = text.split("\r\n");
    if !lines.next()?.starts_with("M-SEARCH * HTTP/1.1") {
        return None;
    }
    let mut target = None;
    let mut delay = 1;
    let mut discover = false;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match name.trim().to_ascii_uppercase().as_str() {
            "ST" => target = Some(value.to_string()),
            "MX" => delay = value.parse().unwrap_or(1),
            "MAN" => discover = value.trim_matches('"') == "ssdp:discover",
            _ => {}
        }
    }
    discover.then_some(())?;
    Some((target?, delay))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_searches() {
        let search = "M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nMX: 2\r\nST: ssdp:all\r\n\r\n";
        assert_eq!(parse_search(search), Some(("ssdp:all".into(), 2)));
        assert_eq!(parse_search("NOTIFY * HTTP/1.1\r\nNT: x\r\n\r\n"), None);
        assert_eq!(
            parse_search("M-SEARCH * HTTP/1.1\r\nST: ssdp:all\r\n\r\n"),
            None
        );
    }
}
