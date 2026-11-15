// SPDX-License-Identifier: AGPL-3.0-only
//! DIAL, the discovery and launch protocol that the YouTube app of a phone
//! uses to find a TV. The service answers for VacuumTube, also while the
//! app is closed.
//!
//! The service knows only whether VacuumTube runs. A launch request starts
//! VacuumTube, waits until the DIAL server of VacuumTube answers, and sends
//! the raw request on to it. VacuumTube does the pairing with YouTube.
//! While VacuumTube runs, the service sends all requests to it.
//!
//! The DIAL server of VacuumTube listens on a different port at each start,
//! and it answers SSDP searches. So the service finds it with the same
//! search that a phone sends. VacuumTube needs its option
//! `device_discoverability` for this. While VacuumTube runs, it answers the
//! searches of phones itself, and the service stays silent. Otherwise the
//! phone would show the HTPC twice. For the same reason, the service uses
//! the name and the UDN of VacuumTube in its own answers, so the phone sees
//! one device, also when VacuumTube is closed. VacuumTube keeps its UDN over
//! restarts, so the service reads both at each start of VacuumTube and
//! saves them (see `identity`).

mod identity;
mod ssdp;
#[cfg(test)]
mod tests;

pub use ssdp::DialSsdp;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use tokio::time::Instant;

use crate::control::Controller;
use crate::dlna::xml::escape;
use crate::dlna::{stable_uuid, xml_response};
use crate::error::Result;
use identity::Identity;

/// The name of the app in the DIAL registry. The phone sends exactly this
/// name, but the service compares without regard to case.
const APP_NAME: &str = "YouTube";
/// VacuumTube needs this long to start and to open its DIAL server.
const START_TIMEOUT: Duration = Duration::from_secs(60);
/// The pause between two searches for VacuumTube
const SEARCH_PAUSE: Duration = Duration::from_millis(500);
const FORWARD_TIMEOUT: Duration = Duration::from_secs(5);
/// A phone asks for the state of the app right after its launch request. It
/// takes the state "stopped" as a failed launch, so the answer waits this
/// long for VacuumTube.
const STATE_WAIT: Duration = Duration::from_secs(12);

/// Starts VacuumTube. The tests replace it, so they start no program.
type Starter = Arc<dyn Fn() -> Pin<Box<dyn Future<Output = Result<()>> + Send>> + Send + Sync>;

pub struct Dial {
    controller: Arc<Controller>,
    identity: Mutex<Identity>,
    identity_file: PathBuf,
    /// Whether the identity of the running VacuumTube is read. The watcher
    /// resets it at each start of VacuumTube.
    identity_read: AtomicBool,
    /// Until this time, a launch request waits for VacuumTube to come up.
    launching_until: Mutex<Option<Instant>>,
    /// Where the search for VacuumTube goes: the SSDP group of the network.
    /// The tests use the address of a fake VacuumTube.
    search_address: SocketAddr,
    starter: Starter,
    port: u16,
    http: reqwest::Client,
}

impl Dial {
    pub fn new(controller: Arc<Controller>, port: u16) -> Arc<Dial> {
        let starter_controller = controller.clone();
        let starter: Starter = Arc::new(move || {
            let controller = starter_controller.clone();
            Box::pin(async move { controller.start_youtube_app().await })
        });
        Dial::with_search_address(controller, port, ssdp::group_address(), starter)
    }

    fn with_search_address(
        controller: Arc<Controller>,
        port: u16,
        search_address: SocketAddr,
        starter: Starter,
    ) -> Arc<Dial> {
        let identity_file = controller
            .config
            .home
            .join(".local/state/x86-linux-htpc/dial-identity.json");
        // Before VacuumTube has run, the name has the form that VacuumTube
        // uses, and the UDN is the own one.
        let identity = Identity::load(&identity_file).unwrap_or_else(|| Identity {
            name: format!("{} (VacuumTube)", controller.config.name),
            udn: format!("uuid:{}", stable_uuid("dial")),
        });
        Arc::new(Dial {
            identity: Mutex::new(identity),
            identity_file,
            identity_read: AtomicBool::new(false),
            launching_until: Mutex::new(None),
            search_address,
            starter,
            controller,
            port,
            http: reqwest::Client::builder()
                .timeout(FORWARD_TIMEOUT)
                .build()
                .expect("the HTTP client starts"),
        })
    }

    pub fn router(self: &Arc<Self>) -> Router {
        Router::new()
            .route("/dial/device.xml", get(device_description))
            .route("/dial/apps/{app}", get(app_state).post(launch))
            .with_state(self.clone())
    }

    fn port(&self) -> u16 {
        self.port
    }

    fn identity(&self) -> Identity {
        self.identity
            .lock()
            .expect("the identity lock works")
            .clone()
    }

    /// The base address of the DIAL server of VacuumTube, or None when
    /// VacuumTube does not run. A search that finds VacuumTube also reads
    /// its identity, the first time after each start of VacuumTube.
    async fn find_vacuumtube(&self) -> Option<String> {
        let base = ssdp::search_vacuumtube(self.search_address).await?;
        if !self.identity_read.swap(true, Ordering::Relaxed) && !self.learn_identity(&base).await {
            // Try again at the next search.
            self.identity_read.store(false, Ordering::Relaxed);
        }
        Some(base)
    }

    /// Takes the name and the UDN from the description of VacuumTube, and
    /// saves them for the times when VacuumTube is closed. Returns false if
    /// VacuumTube gave no identity.
    async fn learn_identity(&self, base: &str) -> bool {
        let Some(learned) = self.read_identity(base).await else {
            return false;
        };
        let previous = self.identity();
        if previous == learned {
            return true;
        }
        // A phone keeps the old UDN for the time of the cache header of
        // our last answer. The goodbye makes it forget the old device now.
        if previous.udn != learned.udn {
            ssdp::send_goodbye(&previous.udn);
            tracing::info!("DIAL: told the network that {} is gone", previous.udn);
        }
        match learned.save(&self.identity_file) {
            Ok(()) => tracing::info!("DIAL: uses the name and the UDN of VacuumTube"),
            Err(error) => tracing::warn!("DIAL: cannot save the identity: {error}"),
        }
        *self.identity.lock().expect("the identity lock works") = learned;
        true
    }

    async fn read_identity(&self, base: &str) -> Option<Identity> {
        let answer = self.http.get(format!("{base}/")).send().await.ok()?;
        Identity::from_description(&answer.text().await.ok()?)
    }

    /// Reads the identity of VacuumTube at each start of VacuumTube, with no
    /// phone involved. The phone needs the identity at the time when
    /// VacuumTube is closed, so it must be known before then.
    pub async fn watch_vacuumtube(self: Arc<Self>) {
        let mut status = self.controller.subscribe();
        let mut was_running = false;
        loop {
            let running = status.borrow_and_update().running.youtube;
            if running && !was_running {
                self.identity_read.store(false, Ordering::Relaxed);
                poll(START_TIMEOUT, || self.find_vacuumtube()).await;
            }
            was_running = running;
            if status.changed().await.is_err() {
                return;
            }
        }
    }

    fn launching(&self) -> bool {
        self.launching_until
            .lock()
            .expect("the launch lock works")
            .is_some_and(|until| Instant::now() < until)
    }

    fn expect_launch(&self) {
        *self.launching_until.lock().expect("the launch lock works") =
            Some(Instant::now() + START_TIMEOUT);
    }

    /// Starts VacuumTube and sends the launch request to it. A phone does
    /// not wait for the start, so this runs after the answer to the phone.
    async fn start_and_forward(&self, headers: HeaderMap, body: Bytes) {
        if let Err(error) = (self.starter)().await {
            tracing::warn!("DIAL: VacuumTube does not start: {error}");
            return;
        }
        self.forward_until_taken(&headers, body).await;
    }

    /// Sends the launch request to VacuumTube until VacuumTube takes it.
    /// The DIAL server of VacuumTube opens before its app page is ready, and
    /// refuses the launch until then. Returns the answer of VacuumTube.
    async fn forward_until_taken(&self, headers: &HeaderMap, body: Bytes) -> Option<StatusCode> {
        let outcome = poll(START_TIMEOUT, || {
            let body = body.clone();
            async move {
                let base = self.find_vacuumtube().await?;
                match self.forward_launch(&base, headers, body).await {
                    Ok(StatusCode::NOT_FOUND | StatusCode::SERVICE_UNAVAILABLE) => None,
                    other => Some(other),
                }
            }
        })
        .await;
        match outcome {
            Some(Ok(status)) => {
                tracing::info!("DIAL: VacuumTube answered the launch with {status}");
                Some(status)
            }
            Some(Err(error)) => {
                tracing::warn!("DIAL: the launch request failed: {error}");
                None
            }
            None => {
                tracing::warn!("DIAL: VacuumTube did not take the launch within {START_TIMEOUT:?}");
                None
            }
        }
    }

    async fn forward_launch(
        &self,
        base: &str,
        headers: &HeaderMap,
        body: Bytes,
    ) -> std::result::Result<StatusCode, reqwest::Error> {
        let mut request = self.http.post(format!("{base}/apps/{APP_NAME}")).body(body);
        if let Some(content_type) = headers.get(header::CONTENT_TYPE) {
            request = request.header(header::CONTENT_TYPE, content_type);
        }
        Ok(request.send().await?.status())
    }

    /// The state of the app from the DIAL server of VacuumTube, with the
    /// address of this service instead of the address of VacuumTube. The
    /// phone must not learn the port of VacuumTube, because it changes at
    /// each start.
    async fn vacuumtube_state(&self, public_base: &str) -> Option<String> {
        let base = self.find_vacuumtube().await?;
        let answer = self
            .http
            .get(format!("{base}/apps/{APP_NAME}"))
            .send()
            .await
            .ok()?;
        if !answer.status().is_success() {
            return None;
        }
        Some(answer.text().await.ok()?.replace(&base, public_base))
    }

    /// The address of the DIAL resources, as the phone reaches this service
    fn public_base(&self, headers: &HeaderMap) -> String {
        let host = headers
            .get(header::HOST)
            .and_then(|host| host.to_str().ok())
            .map_or_else(|| format!("localhost:{}", self.port), String::from);
        format!("http://{host}/dial")
    }
}

/// Calls `attempt` again and again until it gives a value or the time is
/// over. A time of zero gives one attempt.
async fn poll<T, F, Fut>(timeout: Duration, mut attempt: F) -> Option<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Option<T>>,
{
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(value) = attempt().await {
            return Some(value);
        }
        if Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(SEARCH_PAUSE).await;
    }
}

fn is_youtube(app: &str) -> bool {
    app.eq_ignore_ascii_case(APP_NAME)
}

async fn device_description(State(dial): State<Arc<Dial>>, headers: HeaderMap) -> Response {
    let identity = dial.identity();
    let body = format!(
        "<?xml version=\"1.0\"?><root xmlns=\"urn:schemas-upnp-org:device-1-0\">\
         <specVersion><major>1</major><minor>0</minor></specVersion>\
         <device><deviceType>urn:dial-multiscreen-org:device:dial:1</deviceType>\
         <friendlyName>{}</friendlyName><manufacturer>x86-linux-htpc</manufacturer>\
         <modelName>htpc-control</modelName><UDN>{}</UDN></device></root>",
        escape(&identity.name),
        identity.udn
    );
    let mut response = xml_response(body);
    // The phone adds the name of the app to this address.
    if let Ok(value) = HeaderValue::from_str(&format!("{}/apps/", dial.public_base(&headers))) {
        response.headers_mut().insert("Application-URL", value);
    }
    response
}

/// The state of the app. While VacuumTube runs, its own answer has the
/// state "running" and the data of its screen. Otherwise the app is
/// stopped.
async fn app_state(
    State(dial): State<Arc<Dial>>,
    Path(app): Path<String>,
    headers: HeaderMap,
) -> Response {
    if !is_youtube(&app) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let public = dial.public_base(&headers);
    // Right after a launch, the answer waits for VacuumTube.
    let wait = if dial.launching() {
        STATE_WAIT
    } else {
        Duration::ZERO
    };
    if let Some(state) = poll(wait, || dial.vacuumtube_state(&public)).await {
        return xml_response(state);
    }
    xml_response(format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
         <service xmlns=\"urn:dial-multiscreen-org:schemas:dial\"><name>{APP_NAME}</name>\
         <options allowStop=\"false\"/><state>stopped</state></service>"
    ))
}

/// Starts VacuumTube if it does not run, and sends the launch request to
/// it. The answer is 201 with the address of the instance, as DIAL says.
async fn launch(
    State(dial): State<Arc<Dial>>,
    Path(app): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !is_youtube(&app) {
        return StatusCode::NOT_FOUND.into_response();
    }
    tracing::info!("DIAL launch request");
    match dial.find_vacuumtube().await {
        Some(base) => match dial.forward_launch(&base, &headers, body).await {
            Ok(status) if !status.is_success() => {
                tracing::warn!("DIAL: VacuumTube answered the launch with {status}");
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!("DIAL: the launch request failed: {error}");
                return StatusCode::BAD_GATEWAY.into_response();
            }
        },
        None => {
            dial.expect_launch();
            let dial = dial.clone();
            let launch_headers = headers.clone();
            tokio::spawn(async move { dial.start_and_forward(launch_headers, body).await });
        }
    }
    let mut response = StatusCode::CREATED.into_response();
    if let Ok(value) = HeaderValue::from_str(&format!(
        "{}/apps/{APP_NAME}/run",
        dial.public_base(&headers)
    )) {
        response.headers_mut().insert(header::LOCATION, value);
    }
    response
}
