// SPDX-License-Identifier: AGPL-3.0-only
//! htpc-control: the REST API, the DLNA renderer and the DIAL launcher of
//! the HTPC.
//!
//!   htpc-control [serve]   runs the service
//!   htpc-control openapi   prints the OpenAPI document as YAML
//!
//! The parts, from the outside to the inside:
//!
//! - `api`, `dlna` and `dial` take the requests of Home Assistant, the apps
//!   and the phones.
//! - `control` decides which player plays and sends it the commands.
//! - `players`, `apps` and `desktop` talk to the programs on the HTPC.
//! - `status`, `error` and `config` are the types that all parts share.

mod api;
mod apps;
mod config;
mod control;
mod desktop;
mod dial;
mod dlna;
mod error;
mod players;
mod status;
#[cfg(test)]
mod workflow_tests;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use axum::Router;
use tokio::signal::unix::{SignalKind, signal};
use utoipa::OpenApi;

use crate::config::Config;
use crate::control::Controller;
use crate::dial::{Dial, DialSsdp};
use crate::dlna::Dlna;
use crate::dlna::ssdp::{Announcement, Ssdp};

const USAGE: &str = "Usage: htpc-control [serve | openapi | --version]";

#[tokio::main]
async fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        None | Some("serve") => serve().await,
        Some("openapi") => print_openapi(),
        Some("--version") => {
            println!("htpc-control {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("Unknown command {other}. {USAGE}");
            ExitCode::from(2)
        }
    }
}

fn print_openapi() -> ExitCode {
    match api::ApiDoc::openapi().to_yaml() {
        Ok(yaml) => {
            print!("{yaml}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("ERROR: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn serve() -> ExitCode {
    // journald adds the time to each line.
    tracing_subscriber::fmt()
        .without_time()
        .with_target(false)
        .with_ansi(false)
        .init();
    let path = std::env::var_os("HTPC_CONFIG")
        .map_or_else(|| PathBuf::from(config::DEFAULT_CONFIG), PathBuf::from);
    let config = Config::load(&path);
    let port = config.port;
    let dlna_on = config.dlna;
    let config_dial = config.dial && apps::find(&config, "youtube").is_some();
    let controller = Controller::new(config);
    tokio::spawn(controller.clone().poll_forever());

    let mut router = api::router(controller.clone());
    let mut ssdp = None;
    if dlna_on {
        let (dlna_router, dlna_ssdp) = start_dlna(&controller, port);
        router = router.merge(dlna_router);
        ssdp = dlna_ssdp;
    }

    if config_dial {
        router = router.merge(start_dial(&controller, port));
    }

    let listener = match tokio::net::TcpListener::bind(("0.0.0.0", port)).await {
        Ok(listener) => listener,
        Err(error) => {
            tracing::error!("cannot open port {port}: {error}");
            return ExitCode::FAILURE;
        }
    };
    tracing::info!("API on port {port}");
    let result = axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await;
    if let Some(ssdp) = ssdp {
        ssdp.announce(Announcement::Byebye).await;
    }
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!("server failed: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Starts the event sender and the SSDP discovery of the DLNA renderer,
/// and returns the routes of the renderer. Without SSDP, the renderer
/// still works for apps that know its address.
fn start_dlna(controller: &Arc<Controller>, port: u16) -> (Router, Option<Arc<Ssdp>>) {
    let dlna = Dlna::new(controller.clone());
    tokio::spawn(dlna.clone().send_events_forever());
    let ssdp = match Ssdp::bind(dlna.clone(), port) {
        Ok(ssdp) => {
            let ssdp = Arc::new(ssdp);
            tokio::spawn(ssdp.clone().run());
            tracing::info!("DLNA renderer \"{}\" is on", dlna.name);
            Some(ssdp)
        }
        Err(error) => {
            tracing::warn!(
                "SSDP cannot open port 1900, so DLNA apps do not find the renderer: {error}"
            );
            None
        }
    };
    (dlna.router(), ssdp)
}

/// Starts the SSDP answers of DIAL and returns its routes. Without SSDP,
/// the phone cannot find the HTPC, but the routes still work.
fn start_dial(controller: &Arc<Controller>, port: u16) -> Router {
    let dial = Dial::new(controller.clone(), port);
    tokio::spawn(dial.clone().watch_vacuumtube());
    match DialSsdp::bind(dial.clone()) {
        Ok(ssdp) => {
            tokio::spawn(Arc::new(ssdp).run());
            tracing::info!("DIAL for VacuumTube is on");
        }
        Err(error) => tracing::warn!("DIAL cannot open the SSDP port 1900: {error}"),
    }
    dial.router()
}

async fn shutdown_signal() {
    let mut terminate = signal(SignalKind::terminate()).expect("the SIGTERM handler starts");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = terminate.recv() => {}
    }
}
