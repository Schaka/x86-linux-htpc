// SPDX-License-Identifier: AGPL-3.0-only
//! The SSDP side of DIAL: the answers to the searches of phones, the search
//! for VacuumTube, and the goodbye for an old device.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket as StdUdpSocket};
use std::sync::Arc;
use std::time::Duration;

use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;
use tokio::time::Instant;

use super::Dial;
use crate::dlna::ssdp::{Search, parse_search};

/// The search target of DIAL
pub const DIAL_TARGET: &str = "urn:dial-multiscreen-org:service:dial:1";
const GROUP: Ipv4Addr = Ipv4Addr::new(239, 255, 255, 250);
const SSDP_PORT: u16 = 1900;
/// How long a search waits for the answers
const SEARCH_TIME: Duration = Duration::from_millis(800);
/// A pause after a failed receive, so an error that stays does not use the CPU
const ERROR_PAUSE: Duration = Duration::from_secs(1);
/// The USER-AGENT of the own search for VacuumTube. The own search comes
/// back through the multicast group, and the service must not answer it.
pub const OWN_SEARCH_AGENT: &str = "htpc-control-internal-search";

/// The address of the SSDP group of the network. The search for VacuumTube
/// goes there.
pub fn group_address() -> SocketAddr {
    SocketAddr::V4(SocketAddrV4::new(GROUP, SSDP_PORT))
}

/// Sends the DIAL search to the address and returns the base address of the
/// DIAL server of VacuumTube, for example `http://192.0.2.1:40000`. None
/// means that VacuumTube does not answer, so it does not run.
pub async fn search_vacuumtube(address: SocketAddr) -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").await.ok()?;
    let search = format!(
        "M-SEARCH * HTTP/1.1\r\nHOST: {GROUP}:{SSDP_PORT}\r\nMAN: \"ssdp:discover\"\r\n\
         MX: 1\r\nST: {DIAL_TARGET}\r\nUSER-AGENT: {OWN_SEARCH_AGENT}\r\n\r\n"
    );
    socket.send_to(search.as_bytes(), address).await.ok()?;
    let mut buffer = [0u8; 2048];
    let deadline = Instant::now() + SEARCH_TIME;
    while let Ok(Ok((length, _))) =
        tokio::time::timeout_at(deadline, socket.recv_from(&mut buffer)).await
    {
        if let Some(base) = vacuumtube_base(&String::from_utf8_lossy(&buffer[..length])) {
            return Some(base);
        }
    }
    None
}

/// The base address in the answer to a DIAL search of VacuumTube. Other
/// DIAL devices, and this service, are not VacuumTube.
pub(super) fn vacuumtube_base(answer: &str) -> Option<String> {
    let mut location = None;
    let mut is_vacuumtube = false;
    for line in answer.split("\r\n") {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        match name.trim().to_ascii_uppercase().as_str() {
            "LOCATION" => location = Some(value.trim().to_string()),
            "SERVER" => is_vacuumtube = value.contains("VacuumTube"),
            _ => {}
        }
    }
    if !is_vacuumtube {
        return None;
    }
    let location = location?;
    let after_scheme = location.strip_prefix("http://")?;
    let host_port = after_scheme.split('/').next()?;
    Some(format!("http://{host_port}"))
}

/// Answers the DIAL searches of phones on the multicast group. The answer
/// points to the description on the port of the API.
pub struct DialSsdp {
    dial: Arc<Dial>,
    pub(super) socket: UdpSocket,
}

impl DialSsdp {
    pub fn bind(dial: Arc<Dial>) -> std::io::Result<DialSsdp> {
        let any = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, SSDP_PORT));
        DialSsdp::bind_to(dial, any, true)
    }

    /// Listens on the address. With `multicast`, it also joins the SSDP
    /// group. The tests listen on a local address without the group.
    pub(super) fn bind_to(
        dial: Arc<Dial>,
        address: SocketAddr,
        multicast: bool,
    ) -> std::io::Result<DialSsdp> {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        socket.set_reuse_address(true)?;
        // The DLNA renderer and VacuumTube listen on the same port.
        socket.set_reuse_port(true)?;
        socket.bind(&address.into())?;
        if multicast {
            socket.join_multicast_v4(&GROUP, &Ipv4Addr::UNSPECIFIED)?;
        }
        socket.set_nonblocking(true)?;
        Ok(DialSsdp {
            dial,
            socket: UdpSocket::from_std(socket.into())?,
        })
    }

    pub async fn run(self: Arc<Self>) {
        let mut buffer = [0u8; 2048];
        loop {
            let (length, peer) = match self.socket.recv_from(&mut buffer).await {
                Ok(received) => received,
                Err(error) => {
                    tracing::debug!("DIAL SSDP: receive failed: {error}");
                    tokio::time::sleep(ERROR_PAUSE).await;
                    continue;
                }
            };
            let Some(search) = parse_search(&String::from_utf8_lossy(&buffer[..length])) else {
                continue;
            };
            if is_phone_search(&search) {
                let ssdp = self.clone();
                tokio::spawn(async move {
                    // VacuumTube answers by itself while it runs.
                    if ssdp.dial.find_vacuumtube().await.is_none() {
                        ssdp.answer(peer);
                    }
                });
            }
        }
    }

    fn answer(&self, peer: SocketAddr) {
        let Some(local) = local_address_towards(peer) else {
            return;
        };
        let message = search_answer(local, self.dial.port(), &self.dial.identity().udn);
        // A UDP send does not wait, so a plain socket is enough for it.
        if let Ok(reply) = StdUdpSocket::bind("0.0.0.0:0") {
            let _ = reply.send_to(message.as_bytes(), peer);
        }
    }
}

/// Whether the search is a DIAL search of another device
pub(super) fn is_phone_search(search: &Search) -> bool {
    search.target == DIAL_TARGET && search.user_agent.as_deref() != Some(OWN_SEARCH_AGENT)
}

/// The answer to a DIAL search, with the address of the description
pub(super) fn search_answer(local: Ipv4Addr, port: u16, udn: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nCACHE-CONTROL: max-age=1800\r\nEXT:\r\n\
         LOCATION: http://{local}:{port}/dial/device.xml\r\nSERVER: Linux UPnP/1.0 htpc-control/{}\r\n\
         ST: {DIAL_TARGET}\r\nUSN: {udn}::{DIAL_TARGET}\r\n\r\n",
        env!("CARGO_PKG_VERSION")
    )
}

/// Tells the phones that a device with this UDN leaves the network.
pub fn send_goodbye(udn: &str) {
    let Ok(socket) = StdUdpSocket::bind("0.0.0.0:0") else {
        return;
    };
    let _ = socket.set_multicast_ttl_v4(2);
    for (target, usn) in [
        (DIAL_TARGET.to_string(), format!("{udn}::{DIAL_TARGET}")),
        (udn.to_string(), udn.to_string()),
        (
            "upnp:rootdevice".to_string(),
            format!("{udn}::upnp:rootdevice"),
        ),
    ] {
        let message = format!(
            "NOTIFY * HTTP/1.1\r\nHOST: {GROUP}:{SSDP_PORT}\r\nNT: {target}\r\nNTS: ssdp:byebye\r\nUSN: {usn}\r\n\r\n"
        );
        let _ = socket.send_to(message.as_bytes(), group_address());
    }
}

/// The local address that the peer reaches
fn local_address_towards(peer: SocketAddr) -> Option<Ipv4Addr> {
    let probe = StdUdpSocket::bind("0.0.0.0:0").ok()?;
    probe.connect(peer).ok()?;
    match probe.local_addr().ok()?.ip() {
        IpAddr::V4(local) => Some(local),
        IpAddr::V6(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn search(target: &str, user_agent: Option<&str>) -> Search {
        Search {
            target: target.into(),
            max_delay_seconds: 1,
            user_agent: user_agent.map(String::from),
        }
    }

    #[test]
    fn finds_the_vacuumtube_server_in_an_answer() {
        let answer = "HTTP/1.1 200 OK\r\nCACHE-CONTROL: max-age=1800\r\n\
            LOCATION: http://192.0.2.7:38947/\r\nSERVER: linux/7.3 UPnP/1.0 VacuumTube/2.0.1\r\n\
            ST: urn:dial-multiscreen-org:service:dial:1\r\n\r\n";
        assert_eq!(
            vacuumtube_base(answer).as_deref(),
            Some("http://192.0.2.7:38947")
        );
    }

    #[test]
    fn ignores_other_dial_servers() {
        let tv = "HTTP/1.1 200 OK\r\nLocation: http://192.0.2.9:1333/\r\n\
            Server: WebOS/1.5 UPnP/1.0 webOSTV/1.0\r\n\r\n";
        assert_eq!(vacuumtube_base(tv), None);
    }

    #[test]
    fn answers_only_the_dial_searches_of_other_devices() {
        assert!(is_phone_search(&search(DIAL_TARGET, None)));
        assert!(is_phone_search(&search(DIAL_TARGET, Some("Phone/1.0"))));
        // The search of the service itself must not start another search.
        assert!(!is_phone_search(&search(
            DIAL_TARGET,
            Some(OWN_SEARCH_AGENT)
        )));
        assert!(!is_phone_search(&search("ssdp:all", None)));
        assert!(!is_phone_search(&search(
            "urn:schemas-upnp-org:device:MediaRenderer:1",
            None
        )));
    }

    #[test]
    fn the_answer_to_a_search_points_to_the_description() {
        let udn = "uuid:11111111-2222-4333-8444-555555555555";
        let answer = search_answer(Ipv4Addr::new(192, 0, 2, 7), 8780, udn);
        assert!(answer.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(answer.contains("LOCATION: http://192.0.2.7:8780/dial/device.xml\r\n"));
        assert!(answer.contains(&format!("ST: {DIAL_TARGET}\r\n")));
        assert!(answer.contains(&format!("USN: {udn}::{DIAL_TARGET}\r\n")));
        assert!(answer.ends_with("\r\n\r\n"));
        // The own answer is not VacuumTube, or the service would proxy to itself.
        assert_eq!(vacuumtube_base(&answer), None);
    }
}
