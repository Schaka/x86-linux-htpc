// SPDX-License-Identifier: AGPL-3.0-only
//! A UPnP AV media renderer (DLNA DMR) on top of the controller.
//!
//! DLNA knows only "play this address". The renderer gives the address to
//! Controller::play_url, which picks the player. The state that the
//! renderer reports is the state of the player that plays, also for media
//! that another app started, for example a cast from a phone. Home
//! Assistant finds the renderer by SSDP and makes a media player entity
//! for it.

mod description;
pub mod ssdp;
pub mod xml;

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get, post};
use reqwest::Method;
use tokio::sync::Notify;

use crate::control::Controller;
use crate::error::{Error, ErrorKind};
use crate::status::{PlayState, Source, Status};
use description::{SINK_PROTOCOLS, Service};
use xml::{element_text, escape};

/// A start through DLNA shows TRANSITIONING for at most this long, until
/// the player reports the media.
const START_TIMEOUT: Duration = Duration::from_secs(45);
const MAX_SUBSCRIBERS: usize = 32;
const DEFAULT_SUBSCRIPTION: u64 = 1800;
const SUBSCRIPTION_LIMITS: (u64, u64) = (60, 3600);
/// The control point needs the answer to SUBSCRIBE with the SID before the
/// first event.
const FIRST_EVENT_DELAY: Duration = Duration::from_millis(200);
const EVENT_TIMEOUT: Duration = Duration::from_secs(3);
/// After SetAVTransportURI, the renderer reports STOPPED for at most this
/// long, also while another player plays. A control point calls Play only
/// on a renderer that does not play, and Home Assistant reads the state
/// before the player had time to stop.
const SET_TIMEOUT: Duration = Duration::from_secs(60);
const AVT_EVENT_NAMESPACE: &str = "urn:schemas-upnp-org:metadata-1-0/AVT/";

static NOTIFY: LazyLock<Method> =
    LazyLock::new(|| Method::from_bytes(b"NOTIFY").expect("NOTIFY is a valid method"));

pub struct Dlna {
    controller: Arc<Controller>,
    pub udn: String,
    pub name: String,
    http: reqwest::Client,
    renderer: Arc<Mutex<Renderer>>,
    subscribers: Mutex<HashMap<String, Subscriber>>,
    /// Sends an event at once, also when the player state is the same
    changed: Notify,
}

/// The TransportState of AVTransport
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TransportState {
    Stopped,
    Playing,
    PausedPlayback,
    Transitioning,
    NoMediaPresent,
}

impl TransportState {
    fn as_str(self) -> &'static str {
        match self {
            TransportState::Stopped => "STOPPED",
            TransportState::Playing => "PLAYING",
            TransportState::PausedPlayback => "PAUSED_PLAYBACK",
            TransportState::Transitioning => "TRANSITIONING",
            TransportState::NoMediaPresent => "NO_MEDIA_PRESENT",
        }
    }

    /// The value of CurrentTransportActions
    fn actions(self) -> &'static str {
        match self {
            TransportState::Playing | TransportState::Transitioning => "Pause,Stop,Seek",
            TransportState::PausedPlayback => "Play,Stop,Seek",
            TransportState::Stopped => "Play",
            TransportState::NoMediaPresent => "",
        }
    }

    fn has_media(self) -> bool {
        self != TransportState::NoMediaPresent
    }
}

/// What the control point set with SetAVTransportURI
#[derive(Clone, Default)]
struct Renderer {
    uri: String,
    metadata: String,
    /// The renderer started the address, and it still plays
    started: bool,
    starting_since: Option<Instant>,
    /// The time of SetAVTransportURI, until Play or Stop
    set_since: Option<Instant>,
}

impl Renderer {
    fn transport_state(&self, status: &Status) -> TransportState {
        if self
            .set_since
            .is_some_and(|since| since.elapsed() < SET_TIMEOUT)
        {
            return TransportState::Stopped;
        }
        match status.state {
            PlayState::Playing => TransportState::Playing,
            PlayState::Paused => TransportState::PausedPlayback,
            PlayState::Buffering => TransportState::Transitioning,
            PlayState::Idle if self.starting_since.is_some() => TransportState::Transitioning,
            PlayState::Idle if !self.uri.is_empty() => TransportState::Stopped,
            PlayState::Idle => TransportState::NoMediaPresent,
        }
    }

    /// The address and metadata of the media that plays. Media that the
    /// renderer did not start gets metadata from the player.
    fn current_media(&self, status: &Status) -> (String, String) {
        if status.source == Source::Idle || self.set_since.is_some() {
            return (self.uri.clone(), self.metadata.clone());
        }
        let uri = if self.started {
            self.uri.clone()
        } else {
            status.url.clone().unwrap_or_default()
        };
        let metadata = didl(status, &uri);
        (uri, metadata)
    }

    /// Keeps the renderer state in line with the player.
    fn follow_player(&mut self, status: &Status) {
        if status.source != Source::Idle {
            self.starting_since = None;
        } else if self
            .starting_since
            .is_none_or(|since| since.elapsed() > START_TIMEOUT)
        {
            self.starting_since = None;
            self.started = false;
        }
    }
}

struct Subscriber {
    service: Service,
    callback: String,
    expires: Instant,
    seq: u32,
}

/// The output arguments of a SOAP action
type Arguments = Vec<(&'static str, String)>;

impl Dlna {
    pub fn new(controller: Arc<Controller>) -> Arc<Dlna> {
        Arc::new(Dlna {
            udn: format!("uuid:{}", stable_uuid("dlna")),
            name: controller.config.name.clone(),
            controller,
            http: reqwest::Client::builder()
                .timeout(EVENT_TIMEOUT)
                .build()
                .expect("the HTTP client starts"),
            renderer: Arc::new(Mutex::new(Renderer::default())),
            subscribers: Mutex::new(HashMap::new()),
            changed: Notify::new(),
        })
    }

    pub fn router(self: &Arc<Self>) -> Router {
        let router = Router::new().route(
            "/dlna/device.xml",
            get(|State(dlna): State<Arc<Dlna>>| async move {
                xml_response(description::device(&dlna.name, &dlna.udn))
            }),
        );
        Service::ALL
            .into_iter()
            .fold(router, |router, service| {
                let name = service.name();
                router
                    .route(
                        &format!("/dlna/{name}.xml"),
                        get(move || async move { xml_response(service.description()) }),
                    )
                    .route(
                        &format!("/dlna/{name}/control"),
                        post(move |dlna, headers, body| control(dlna, service, headers, body)),
                    )
                    .route(
                        &format!("/dlna/{name}/event"),
                        any(move |dlna, request| subscription(dlna, service, request)),
                    )
            })
            .with_state(self.clone())
    }

    fn renderer(&self) -> Renderer {
        self.renderer.lock().unwrap().clone()
    }

    /// Sends an event to the subscribers of AVTransport when the media or
    /// the state changes. The position changes each second, and control
    /// points ask for it with GetPositionInfo.
    pub async fn send_events_forever(self: Arc<Self>) {
        let mut updates = self.controller.subscribe();
        let mut last: Option<Status> = None;
        loop {
            tokio::select! {
                result = updates.changed() => if result.is_err() { return },
                () = self.changed.notified() => last = None,
            }
            let status = updates.borrow_and_update().clone();
            self.renderer.lock().unwrap().follow_player(&status);
            if last
                .as_ref()
                .is_some_and(|last| last.same_media_and_state(&status))
            {
                continue;
            }
            let last_change = self.av_last_change(&status);
            last = Some(status);
            self.notify(
                |_, subscriber| subscriber.service == Service::AvTransport,
                &[("LastChange", last_change)],
            );
        }
    }

    fn av_last_change(&self, status: &Status) -> String {
        let renderer = self.renderer();
        let state = renderer.transport_state(status);
        let (uri, metadata) = renderer.current_media(status);
        let duration = hms(status.duration.unwrap_or(0.0));
        let values = [
            ("TransportState", state.as_str().to_string()),
            ("TransportStatus", "OK".into()),
            ("CurrentTransportActions", state.actions().into()),
            ("AVTransportURI", uri.clone()),
            ("AVTransportURIMetaData", metadata.clone()),
            ("CurrentTrackURI", uri),
            ("CurrentTrackMetaData", metadata),
            ("CurrentMediaDuration", duration.clone()),
            ("CurrentTrackDuration", duration),
            ("NumberOfTracks", track_count(state.has_media()).into()),
            ("CurrentPlayMode", "NORMAL".into()),
            ("TransportPlaySpeed", "1".into()),
        ];
        last_change(AVT_EVENT_NAMESPACE, &values)
    }

    /// The first event of a subscription has all evented values.
    fn initial_event(&self, service: Service) -> Arguments {
        match service {
            Service::AvTransport => {
                vec![("LastChange", self.av_last_change(&self.controller.status()))]
            }
            // The renderer has no rendering values. An InstanceID without
            // values makes control points warn, so the event is empty.
            Service::RenderingControl => vec![("LastChange", String::new())],
            Service::ConnectionManager => vec![
                ("SourceProtocolInfo", String::new()),
                ("SinkProtocolInfo", SINK_PROTOCOLS.into()),
                ("CurrentConnectionIDs", "0".into()),
            ],
        }
    }

    /// Sends an event to each subscriber that `select` takes.
    fn notify(&self, select: impl Fn(&str, &Subscriber) -> bool, properties: &[(&str, String)]) {
        let body = property_set(properties);
        let mut subscribers = self.subscribers.lock().unwrap();
        subscribers.retain(|_, subscriber| subscriber.expires > Instant::now());
        for (sid, subscriber) in subscribers
            .iter_mut()
            .filter(|(sid, subscriber)| select(sid, subscriber))
        {
            let request = self
                .http
                .request(NOTIFY.clone(), &subscriber.callback)
                .header("Content-Type", "text/xml; charset=\"utf-8\"")
                .header("NT", "upnp:event")
                .header("NTS", "upnp:propchange")
                .header("SID", sid.as_str())
                .header("SEQ", subscriber.seq.to_string())
                .body(body.clone());
            // The first event has SEQ 0. After the wrap, SEQ starts at 1.
            subscriber.seq = subscriber.seq.wrapping_add(1).max(1);
            let callback = subscriber.callback.clone();
            tokio::spawn(async move {
                if let Err(error) = request.send().await {
                    tracing::debug!("DLNA event to {callback} failed: {error}");
                }
            });
        }
    }

    async fn av_transport(&self, action: &str, body: &str) -> Result<Arguments, Fault> {
        let status = self.controller.status();
        let renderer = self.renderer();
        let state = renderer.transport_state(&status);
        let (uri, metadata) = renderer.current_media(&status);
        let duration = hms(status.duration.unwrap_or(0.0));
        match action {
            "SetAVTransportURI" => {
                self.set_uri(
                    element_text(body, "CurrentURI").unwrap_or_default(),
                    element_text(body, "CurrentURIMetaData").unwrap_or_default(),
                );
                Ok(vec![])
            }
            "Play" if !renderer.uri.is_empty() && !renderer.started => {
                self.start(renderer);
                Ok(vec![])
            }
            "Play" => self.set_paused(false).await,
            "Pause" => self.set_paused(true).await,
            "Stop" => self.stop().await,
            "Seek" => self.seek(body).await,
            "GetTransportInfo" => Ok(vec![
                ("CurrentTransportState", state.as_str().into()),
                ("CurrentTransportStatus", "OK".into()),
                ("CurrentSpeed", "1".into()),
            ]),
            "GetPositionInfo" => {
                let position = hms(status.position.unwrap_or(0.0));
                Ok(vec![
                    ("Track", track_count(!uri.is_empty()).into()),
                    ("TrackDuration", duration),
                    ("TrackMetaData", metadata),
                    ("TrackURI", uri),
                    ("RelTime", position.clone()),
                    ("AbsTime", position),
                    ("RelCount", i32::MAX.to_string()),
                    ("AbsCount", i32::MAX.to_string()),
                ])
            }
            "GetMediaInfo" => Ok(vec![
                ("NrTracks", track_count(!uri.is_empty()).into()),
                ("MediaDuration", duration),
                ("CurrentURI", uri),
                ("CurrentURIMetaData", metadata),
                ("NextURI", String::new()),
                ("NextURIMetaData", String::new()),
                (
                    "PlayMedium",
                    if state.has_media() { "NETWORK" } else { "NONE" }.into(),
                ),
                ("RecordMedium", "NOT_IMPLEMENTED".into()),
                ("WriteStatus", "NOT_IMPLEMENTED".into()),
            ]),
            "GetTransportSettings" => Ok(vec![
                ("PlayMode", "NORMAL".into()),
                ("RecQualityMode", "NOT_IMPLEMENTED".into()),
            ]),
            "GetCurrentTransportActions" => Ok(vec![("Actions", state.actions().into())]),
            "GetDeviceCapabilities" => Ok(vec![
                ("PlayMedia", "NETWORK".into()),
                ("RecMedia", "NOT_IMPLEMENTED".into()),
                ("RecQualityModes", "NOT_IMPLEMENTED".into()),
            ]),
            _ => Err(Fault::INVALID_ACTION),
        }
    }

    fn set_uri(&self, uri: String, metadata: String) {
        tracing::info!("DLNA address {uri}");
        *self.renderer.lock().unwrap() = Renderer {
            uri,
            metadata,
            set_since: Some(Instant::now()),
            ..Renderer::default()
        };
        self.changed.notify_one();
    }

    /// The start can take some seconds, for example when Kodi must end
    /// first. The answer to Play goes out at once, and the state shows
    /// TRANSITIONING until the player has the media.
    fn start(&self, renderer: Renderer) {
        {
            let mut current = self.renderer.lock().unwrap();
            current.started = true;
            current.starting_since = Some(Instant::now());
            current.set_since = None;
        }
        self.changed.notify_one();
        let controller = self.controller.clone();
        let title = element_text(&renderer.metadata, "title");
        let audio = is_audio_item(&renderer.metadata);
        let shared = self.renderer.clone();
        tokio::spawn(async move {
            if let Err(error) = controller
                .play_url(&renderer.uri, title.as_deref(), audio)
                .await
            {
                tracing::warn!("DLNA start of {} failed: {error}", renderer.uri);
                let mut current = shared.lock().unwrap();
                current.started = false;
                current.starting_since = None;
            }
        });
    }

    async fn set_paused(&self, paused: bool) -> Result<Arguments, Fault> {
        self.controller.set_paused(Some(paused)).await?;
        Ok(vec![])
    }

    /// A Stop without media is no error, because the control point only
    /// wants the renderer to be stopped.
    async fn stop(&self) -> Result<Arguments, Fault> {
        {
            let mut current = self.renderer.lock().unwrap();
            current.started = false;
            current.set_since = None;
        }
        match self.controller.stop().await {
            Err(error) if error.kind != ErrorKind::NothingPlays => return Err(error.into()),
            _ => {}
        }
        self.changed.notify_one();
        Ok(vec![])
    }

    async fn seek(&self, body: &str) -> Result<Arguments, Fault> {
        let unit = element_text(body, "Unit").unwrap_or_default();
        if unit != "REL_TIME" && unit != "ABS_TIME" {
            return Err(Fault::new(710, "Seek mode not supported"));
        }
        let target = element_text(body, "Target").unwrap_or_default();
        let position = parse_hms(&target).ok_or(Fault::new(711, "Illegal seek target"))?;
        self.controller.seek(position).await?;
        Ok(vec![])
    }

    fn connection_manager(action: &str) -> Result<Arguments, Fault> {
        match action {
            "GetProtocolInfo" => Ok(vec![
                ("Source", String::new()),
                ("Sink", SINK_PROTOCOLS.into()),
            ]),
            "GetCurrentConnectionIDs" => Ok(vec![("ConnectionIDs", "0".into())]),
            "GetCurrentConnectionInfo" => Ok(vec![
                ("RcsID", "0".into()),
                ("AVTransportID", "0".into()),
                ("ProtocolInfo", String::new()),
                ("PeerConnectionManager", String::new()),
                ("PeerConnectionID", "-1".into()),
                ("Direction", "Input".into()),
                ("Status", "OK".into()),
            ]),
            _ => Err(Fault::INVALID_ACTION),
        }
    }
}

/// "1" when the renderer has a track, else "0"
fn track_count(has_track: bool) -> &'static str {
    if has_track { "1" } else { "0" }
}

/// A UPnP error answer
struct Fault {
    code: u16,
    description: &'static str,
}

impl Fault {
    const INVALID_ACTION: Fault = Fault::new(401, "Invalid Action");

    const fn new(code: u16, description: &'static str) -> Fault {
        Fault { code, description }
    }
}

impl From<Error> for Fault {
    fn from(error: Error) -> Fault {
        tracing::info!("DLNA action failed: {error}");
        match error.kind {
            ErrorKind::NothingPlays => Fault::new(701, "Transition not available"),
            ErrorKind::BadRequest => Fault::new(402, "Invalid Args"),
            _ => Fault::new(501, "Action Failed"),
        }
    }
}

async fn control(
    State(dlna): State<Arc<Dlna>>,
    service: Service,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let body = String::from_utf8_lossy(&body);
    let action = headers
        .get("soapaction")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim_matches('"'))
        .and_then(|value| value.rsplit_once('#'))
        .map(|(_, action)| action.to_string())
        .unwrap_or_default();
    let result = match service {
        Service::AvTransport => dlna.av_transport(&action, &body).await,
        Service::RenderingControl => Err(Fault::INVALID_ACTION),
        Service::ConnectionManager => Dlna::connection_manager(&action),
    };
    match result {
        Ok(arguments) => soap_answer(service, &action, &arguments),
        Err(fault) => soap_fault(&fault),
    }
}

fn soap_answer(service: Service, action: &str, arguments: &[(&str, String)]) -> Response {
    let arguments: String = arguments
        .iter()
        .map(|(name, value)| format!("<{name}>{}</{name}>", escape(value)))
        .collect();
    let body = format!(
        "<u:{action}Response xmlns:u=\"{}\">{arguments}</u:{action}Response>",
        service.urn()
    );
    xml_response(envelope(&body))
}

/// UPnP sends an error as a SOAP fault with HTTP status 500.
fn soap_fault(fault: &Fault) -> Response {
    let body = format!(
        "<s:Fault><faultcode>s:Client</faultcode><faultstring>UPnPError</faultstring><detail>\
         <UPnPError xmlns=\"urn:schemas-upnp-org:control-1-0\"><errorCode>{}</errorCode>\
         <errorDescription>{}</errorDescription></UPnPError></detail></s:Fault>",
        fault.code, fault.description
    );
    let mut response = xml_response(envelope(&body));
    *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
    response
}

/// SUBSCRIBE and UNSUBSCRIBE of the UPnP event protocol (GENA)
async fn subscription(
    State(dlna): State<Arc<Dlna>>,
    service: Service,
    request: Request,
) -> Response {
    let headers = request.headers();
    let header = |name: &str| headers.get(name).and_then(|value| value.to_str().ok());
    let seconds = header("timeout")
        .and_then(|timeout| timeout.strip_prefix("Second-"))
        .and_then(|seconds| seconds.parse::<u64>().ok())
        .unwrap_or(DEFAULT_SUBSCRIPTION)
        .clamp(SUBSCRIPTION_LIMITS.0, SUBSCRIPTION_LIMITS.1);
    let lifetime = Duration::from_secs(seconds);
    let sid = header("sid");
    let answer = match (request.method().as_str(), sid) {
        ("SUBSCRIBE", Some(sid)) => dlna.renew(sid, lifetime).map(|()| sid.to_string()),
        ("SUBSCRIBE", None) => {
            // The callback header has the form <http://host/path>.
            let callback = header("callback")
                .and_then(|callback| callback.split('>').next())
                .map(|callback| callback.trim_start_matches('<'))
                .filter(|callback| callback.starts_with("http://"));
            match (callback, header("nt")) {
                (Some(callback), Some("upnp:event")) => {
                    dlna.subscribe(service, callback.to_string(), lifetime)
                }
                _ => Err(StatusCode::PRECONDITION_FAILED),
            }
        }
        ("UNSUBSCRIBE", sid) => {
            return dlna.unsubscribe(sid).into_response();
        }
        _ => Err(StatusCode::METHOD_NOT_ALLOWED),
    };
    match answer {
        Ok(sid) => subscribed(&sid, seconds),
        Err(status) => status.into_response(),
    }
}

impl Dlna {
    fn renew(&self, sid: &str, lifetime: Duration) -> Result<(), StatusCode> {
        let mut subscribers = self.subscribers.lock().unwrap();
        let subscriber = subscribers
            .get_mut(sid)
            .ok_or(StatusCode::PRECONDITION_FAILED)?;
        subscriber.expires = Instant::now() + lifetime;
        Ok(())
    }

    /// Adds a subscriber and returns its SID. The first event follows
    /// the answer, with all evented values.
    fn subscribe(
        self: &Arc<Self>,
        service: Service,
        callback: String,
        lifetime: Duration,
    ) -> Result<String, StatusCode> {
        let sid = format!("uuid:{}", random_uuid());
        {
            let mut subscribers = self.subscribers.lock().unwrap();
            subscribers.retain(|_, subscriber| subscriber.expires > Instant::now());
            if subscribers.len() >= MAX_SUBSCRIBERS {
                return Err(StatusCode::SERVICE_UNAVAILABLE);
            }
            subscribers.insert(
                sid.clone(),
                Subscriber {
                    service,
                    callback,
                    expires: Instant::now() + lifetime,
                    seq: 0,
                },
            );
        }
        let dlna = self.clone();
        let first_sid = sid.clone();
        tokio::spawn(async move {
            tokio::time::sleep(FIRST_EVENT_DELAY).await;
            let properties = dlna.initial_event(service);
            dlna.notify(|sid, _| sid == first_sid, &properties);
        });
        Ok(sid)
    }

    fn unsubscribe(&self, sid: Option<&str>) -> StatusCode {
        let removed = sid.and_then(|sid| self.subscribers.lock().unwrap().remove(sid));
        if removed.is_some() {
            StatusCode::OK
        } else {
            StatusCode::PRECONDITION_FAILED
        }
    }
}

fn subscribed(sid: &str, seconds: u64) -> Response {
    let mut response = StatusCode::OK.into_response();
    let headers = response.headers_mut();
    headers.insert("SID", HeaderValue::from_str(sid).expect("the SID is ASCII"));
    headers.insert(
        "TIMEOUT",
        HeaderValue::from_str(&format!("Second-{seconds}")).expect("ASCII"),
    );
    response
}

/// Whether the DIDL-Lite metadata describes an audio item. A DLNA app of a
/// phone sets the class `object.item.audioItem` for music.
fn is_audio_item(metadata: &str) -> bool {
    element_text(metadata, "class").is_some_and(|class| class.starts_with("object.item.audioItem"))
}

pub fn xml_response(body: String) -> Response {
    let mut response = body.into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/xml; charset=\"utf-8\""),
    );
    response
}

fn envelope(body: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" \
         s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><s:Body>{body}</s:Body></s:Envelope>"
    )
}

/// The LastChange value: an XML document as text, so it is escaped once
/// here and once more in the property set.
fn last_change(namespace: &str, values: &[(&str, String)]) -> String {
    let fields: String = values
        .iter()
        .map(|(k, v)| format!("<{k} val=\"{}\"/>", escape(v)))
        .collect();
    format!("<Event xmlns=\"{namespace}\"><InstanceID val=\"0\">{fields}</InstanceID></Event>")
}

fn property_set(properties: &[(&str, String)]) -> String {
    let fields: String = properties
        .iter()
        .map(|(k, v)| format!("<e:property><{k}>{}</{k}></e:property>", escape(v)))
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<e:propertyset xmlns:e=\"urn:schemas-upnp-org:event-1-0\">{fields}</e:propertyset>"
    )
}

/// DIDL-Lite metadata of the media that plays
fn didl(status: &Status, uri: &str) -> String {
    let mut item = String::new();
    if let Some(title) = &status.title {
        item.push_str(&format!("<dc:title>{}</dc:title>", escape(title)));
    }
    if let Some(subtitle) = &status.subtitle {
        item.push_str(&format!(
            "<dc:creator>{0}</dc:creator><upnp:artist>{0}</upnp:artist>",
            escape(subtitle)
        ));
    }
    if let Some(image) = &status.image {
        item.push_str(&format!(
            "<upnp:albumArtURI>{}</upnp:albumArtURI>",
            escape(image)
        ));
    }
    item.push_str("<upnp:class>object.item.videoItem</upnp:class>");
    let duration = status
        .duration
        .map(|d| format!(" duration=\"{}\"", hms(d)))
        .unwrap_or_default();
    item.push_str(&format!(
        "<res protocolInfo=\"http-get:*:*:*\"{duration}>{}</res>",
        escape(uri)
    ));
    format!(
        "<DIDL-Lite xmlns=\"urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" \
         xmlns:upnp=\"urn:schemas-upnp-org:metadata-1-0/upnp/\"><item id=\"0\" parentID=\"-1\" restricted=\"1\">{item}</item></DIDL-Lite>"
    )
}

/// H:MM:SS, the time format of UPnP
fn hms(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    format!("{}:{:02}:{:02}", total / 3600, total / 60 % 60, total % 60)
}

/// H:MM:SS with an optional fraction, or MM:SS
fn parse_hms(text: &str) -> Option<f64> {
    let parts: Vec<f64> = text
        .trim()
        .split(':')
        .map(|p| p.parse::<f64>().ok())
        .collect::<Option<_>>()?;
    match parts.as_slice() {
        [h, m, s] => Some(h * 3600.0 + m * 60.0 + s),
        [m, s] => Some(m * 60.0 + s),
        [s] => Some(*s),
        _ => None,
    }
}

fn format_uuid(bytes: [u8; 16]) -> String {
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

fn random_uuid() -> String {
    use std::io::Read;
    let mut bytes = [0u8; 16];
    let read = std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut bytes));
    if read.is_err() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        bytes = now.to_le_bytes();
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format_uuid(bytes)
}

/// The UDN must stay the same over restarts, or Home Assistant sees a new
/// device. It comes from a hash of the machine ID, so the machine ID itself
/// does not go out on the network.
///
/// The kind keeps the UDNs of different services on this machine apart.
pub fn stable_uuid(kind: &str) -> String {
    let machine_id =
        std::fs::read_to_string("/etc/machine-id").unwrap_or_else(|_| "x86-linux-htpc".into());
    let hash = |salt: &str| {
        // FNV-1a, 64 bit
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in salt.bytes().chain(machine_id.trim().bytes()) {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        h
    };
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&hash(&format!("htpc-control-{kind}-a")).to_be_bytes());
    bytes[8..].copy_from_slice(&hash(&format!("htpc-control-{kind}-b")).to_be_bytes());
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format_uuid(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_times() {
        assert_eq!(hms(3725.9), "1:02:05");
        assert_eq!(parse_hms("1:02:05.500"), Some(3725.5));
        assert_eq!(parse_hms("02:05"), Some(125.0));
        assert_eq!(parse_hms("x"), None);
    }

    #[test]
    fn escapes_last_change_twice() {
        let change = last_change("urn:x", &[("AVTransportURI", "http://h/a?b=1&c=2".into())]);
        assert!(change.contains("val=\"http://h/a?b=1&amp;c=2\""));
        let set = property_set(&[("LastChange", change)]);
        assert!(set.contains("&lt;Event") && set.contains("&amp;amp;c=2"));
    }

    #[test]
    fn reports_the_transport_state() {
        let playing = Status {
            source: Source::Url,
            state: PlayState::Playing,
            ..Status::default()
        };
        let idle = Status::default();
        let empty = Renderer::default();
        assert_eq!(empty.transport_state(&idle), TransportState::NoMediaPresent);
        assert_eq!(empty.transport_state(&playing), TransportState::Playing);

        let set = Renderer {
            uri: "http://h/a.mkv".into(),
            set_since: Some(Instant::now()),
            ..Renderer::default()
        };
        assert_eq!(set.transport_state(&playing), TransportState::Stopped);

        let starting = Renderer {
            uri: "http://h/a.mkv".into(),
            started: true,
            starting_since: Some(Instant::now()),
            ..Renderer::default()
        };
        assert_eq!(
            starting.transport_state(&idle),
            TransportState::Transitioning
        );
        assert_eq!(TransportState::Stopped.actions(), "Play");
    }

    #[test]
    fn knows_audio_items_by_their_class() {
        let item = |class: &str| {
            format!(
                "<DIDL-Lite xmlns:upnp=\"urn:schemas-upnp-org:metadata-1-0/upnp/\"><item>\
                 <upnp:class>{class}</upnp:class></item></DIDL-Lite>"
            )
        };
        assert!(is_audio_item(&item("object.item.audioItem.musicTrack")));
        assert!(is_audio_item(&item("object.item.audioItem")));
        assert!(!is_audio_item(&item("object.item.videoItem.movie")));
        assert!(!is_audio_item(""));
        assert!(!is_audio_item("not xml"));
    }

    #[test]
    fn keeps_the_udn() {
        assert_eq!(stable_uuid("dlna"), stable_uuid("dlna"));
        assert_ne!(stable_uuid("dlna"), stable_uuid("dial"));
        assert_ne!(random_uuid(), random_uuid());
    }
}
