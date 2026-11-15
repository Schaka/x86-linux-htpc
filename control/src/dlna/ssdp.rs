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
use super::description::{DEVICE_TYPE, Service};

const GROUP: Ipv4Addr = Ipv4Addr::new(239, 255, 255, 250);
const PORT: u16 = 1900;
const MAX_AGE: u64 = 1800;
const ANNOUNCE_INTERVAL: Duration = Duration::from_secs(600);
/// The UPnP specification allows an answer delay of up to MX seconds. A
/// short delay keeps the discovery in the apps fast.
const MAX_ANSWER_DELAY: Duration = Duration::from_secs(1);

/// The kind of a NOTIFY message
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Announcement {
    /// The renderer is on the network.
    Alive,
    /// The renderer leaves the network.
    Byebye,
}

impl Announcement {
    fn nts(self) -> &'static str {
        match self {
            Announcement::Alive => "ssdp:alive",
            Announcement::Byebye => "ssdp:byebye",
        }
    }
}

/// A search request of a control point
#[derive(Debug, PartialEq, Eq)]
pub struct Search {
    pub target: String,
    /// The MX header: the answer can wait up to this many seconds.
    pub max_delay_seconds: u64,
    /// The USER-AGENT header, if the search has it
    pub user_agent: Option<String>,
}

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
        targets.extend(Service::ALL.iter().map(|service| {
            let urn = service.urn();
            (urn.to_string(), format!("{udn}::{urn}"))
        }));
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
                announcer.announce(Announcement::Alive).await;
                tokio::time::sleep(ANNOUNCE_INTERVAL).await;
            }
        });
        let mut buffer = [0u8; 2048];
        loop {
            let (length, peer) = match self.socket.recv_from(&mut buffer).await {
                Ok(received) => received,
                Err(error) => {
                    tracing::debug!("SSDP: receive failed: {error}");
                    continue;
                }
            };
            if let Some(search) = parse_search(&String::from_utf8_lossy(&buffer[..length])) {
                let ssdp = self.clone();
                tokio::spawn(async move { ssdp.answer(peer, search).await });
            }
        }
    }

    async fn answer(&self, peer: SocketAddr, search: Search) {
        let Some(location) = self.location(peer) else {
            return;
        };
        tokio::time::sleep(answer_delay(peer, search.max_delay_seconds)).await;
        for (target, usn) in self.targets() {
            if search.target != "ssdp:all" && search.target != target {
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

    /// Sends the announcement for all targets.
    pub async fn announce(&self, kind: Announcement) {
        let group = SocketAddr::V4(SocketAddrV4::new(GROUP, PORT));
        let Some(location) = self.location(group) else {
            tracing::debug!("SSDP: no network for the announcement");
            return;
        };
        for (target, usn) in self.targets() {
            let mut message = format!(
                "NOTIFY * HTTP/1.1\r\nHOST: {GROUP}:{PORT}\r\nNT: {target}\r\nNTS: {}\r\nUSN: {usn}\r\n",
                kind.nts()
            );
            if kind == Announcement::Alive {
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

/// A delay below MX spreads the answers of all devices on the network.
/// The process ID and the port of the peer vary enough for this, so the
/// renderer needs no random number generator.
fn answer_delay(peer: SocketAddr, max_delay_seconds: u64) -> Duration {
    let limit = Duration::from_secs(max_delay_seconds.max(1)).min(MAX_ANSWER_DELAY);
    let seed = u64::from(std::process::id()) * 7919 + u64::from(peer.port());
    let limit_millis = u64::try_from(limit.as_millis()).unwrap_or(1000);
    Duration::from_millis(seed % limit_millis)
}

/// The search of an M-SEARCH request
pub fn parse_search(text: &str) -> Option<Search> {
    let mut lines = text.split("\r\n");
    if !lines.next()?.starts_with("M-SEARCH * HTTP/1.1") {
        return None;
    }
    let mut target = None;
    let mut delay = 1;
    let mut discover = false;
    let mut user_agent = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match name.trim().to_ascii_uppercase().as_str() {
            "ST" => target = Some(value.to_string()),
            "MX" => delay = value.parse().unwrap_or(1),
            "MAN" => discover = value.trim_matches('"') == "ssdp:discover",
            "USER-AGENT" => user_agent = Some(value.to_string()),
            _ => {}
        }
    }
    discover.then_some(Search {
        target: target?,
        max_delay_seconds: delay,
        user_agent,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_within_the_delay_limit() {
        let peer: SocketAddr = "192.0.2.1:50000".parse().unwrap();
        for mx in [0, 1, 3, 120] {
            assert!(answer_delay(peer, mx) < MAX_ANSWER_DELAY);
        }
    }

    #[test]
    fn parses_the_user_agent_of_a_search() {
        let search = "M-SEARCH * HTTP/1.1\r\nMAN: \"ssdp:discover\"\r\nST: ssdp:all\r\nUSER-AGENT: Phone/1.0\r\n\r\n";
        assert_eq!(
            parse_search(search).and_then(|s| s.user_agent).as_deref(),
            Some("Phone/1.0")
        );
    }

    #[test]
    fn parses_searches() {
        let search = "M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nMX: 2\r\nST: ssdp:all\r\n\r\n";
        assert_eq!(
            parse_search(search),
            Some(Search {
                target: "ssdp:all".into(),
                max_delay_seconds: 2,
                user_agent: None
            })
        );
        assert_eq!(parse_search("NOTIFY * HTTP/1.1\r\nNT: x\r\n\r\n"), None);
        assert_eq!(
            parse_search("M-SEARCH * HTTP/1.1\r\nST: ssdp:all\r\n\r\n"),
            None
        );
    }
}
