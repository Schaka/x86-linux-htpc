// SPDX-License-Identifier: AGPL-3.0-only
//! The clients of the players. Each client reads the state of one player
//! as a Status and sends it the play keys. The controller decides which
//! player gets a command.

pub mod jellyfin;
pub mod kodi;
pub mod mpv;
pub mod youtube;

use std::time::Duration;

use tokio::net::TcpStream;

/// A player that runs answers on localhost at once. A short limit keeps
/// the poll of all players fast when one of them does not run.
const PORT_CHECK_TIMEOUT: Duration = Duration::from_millis(300);

/// True when a program listens on this port of localhost
async fn port_open(port: u16) -> bool {
    matches!(
        tokio::time::timeout(PORT_CHECK_TIMEOUT, TcpStream::connect(("127.0.0.1", port))).await,
        Ok(Ok(_))
    )
}
