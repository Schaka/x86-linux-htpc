// SPDX-License-Identifier: AGPL-3.0-only
//! htpc-control: the REST API and the DLNA renderer of the HTPC.
//!
//!   htpc-control [serve]   runs the service
//!   htpc-control openapi   prints the OpenAPI document as YAML

mod api;
mod apps;
mod config;
mod control;
mod dlna;
mod error;
mod jellyfin;
mod kodi;
mod mpv;
mod status;
mod youtube;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use utoipa::OpenApi;

#[tokio::main]
async fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        None | Some("serve") => serve().await,
        Some("openapi") => match api::ApiDoc::openapi().to_yaml() {
            Ok(yaml) => {
                print!("{yaml}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("ERROR: {error}");
                ExitCode::FAILURE
            }
        },
        Some("--version") => {
            println!("htpc-control {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("Unknown command {other}. Usage: htpc-control [serve | openapi | --version]");
            ExitCode::from(2)
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
        .map(PathBuf::from)
        .unwrap_or_else(|| config::DEFAULT_CONFIG.into());
    let config = config::Config::load(&path);
    let port = config.port;
    let dlna_on = config.dlna;
    let controller = control::Controller::new(config);
    tokio::spawn(controller.clone().poll_forever());

    let mut router = api::router(controller.clone());
    let mut ssdp = None;
    if dlna_on {
        let dlna = dlna::Dlna::new(controller.clone());
        router = router.merge(dlna.router());
        tokio::spawn(dlna.clone().send_events_forever());
        match dlna::ssdp::Ssdp::bind(dlna.clone(), port) {
            Ok(s) => {
                let s = Arc::new(s);
                tokio::spawn(s.clone().run());
                ssdp = Some(s);
                tracing::info!("DLNA renderer \"{}\" is on", dlna.name);
            }
            Err(error) => tracing::warn!(
                "SSDP cannot open port 1900, so DLNA apps do not find the renderer: {error}"
            ),
        }
    }

    let listener = match tokio::net::TcpListener::bind(("0.0.0.0", port)).await {
        Ok(l) => l,
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
        ssdp.announce("ssdp:byebye").await;
    }
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!("server failed: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn shutdown_signal() {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("signal handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = terminate.recv() => {}
    }
}
