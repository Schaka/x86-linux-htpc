// SPDX-License-Identifier: AGPL-3.0-only
//! Tests of the DIAL launcher with a fake VacuumTube and a fake phone.

use super::ssdp::DIAL_TARGET;
use super::*;
use crate::config::Config;
use crate::dlna::xml::element_text;
use crate::status::{Running, Status};
use tokio::net::UdpSocket;

/// The content type and the body of each launch request that a fake
/// VacuumTube took
type Launches = Arc<Mutex<Vec<(Option<String>, String)>>>;

/// A VacuumTube for the tests: an SSDP responder, and a DIAL server that
/// records the launch requests.
struct FakeVacuumTube {
    /// Where the search of the service goes
    search: SocketAddr,
    base: String,
    /// The content type and the body of each launch request that it took
    launches: Launches,
    /// Whether VacuumTube answers SSDP searches, as it does while it runs
    running: Arc<std::sync::atomic::AtomicBool>,
}

#[derive(Clone)]
struct FakeState {
    base: String,
    launches: Launches,
    /// The first launches that the server refuses, as VacuumTube does
    /// until its app page is ready
    refuse: Arc<Mutex<usize>>,
}

const FAKE_UDN: &str = "uuid:11111111-2222-4333-8444-555555555555";
const FAKE_NAME: &str = "Test HTPC (VacuumTube)";

/// A VacuumTube that runs
async fn fake_vacuumtube(refuse_launches: usize) -> FakeVacuumTube {
    fake_vacuumtube_with(refuse_launches, true).await
}

async fn fake_vacuumtube_with(refuse_launches: usize, running: bool) -> FakeVacuumTube {
    let running = Arc::new(AtomicBool::new(running));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let launches = Arc::new(Mutex::new(Vec::new()));
    let state = FakeState {
        base: base.clone(),
        launches: launches.clone(),
        refuse: Arc::new(Mutex::new(refuse_launches)),
    };
    let router = Router::new()
        .route(
            "/",
            get(|| async {
                xml_response(format!(
                    "<root><device><friendlyName>{FAKE_NAME}</friendlyName>\
                     <UDN>{FAKE_UDN}</UDN></device></root>"
                ))
            }),
        )
        .route(
            "/apps/YouTube",
            get(|State(state): State<FakeState>| async move {
                xml_response(format!(
                    "<service><name>YouTube</name><state>running</state>\
                     <link rel=\"run\" href=\"{}/apps/YouTube/run\"/></service>",
                    state.base
                ))
            })
            .post(
                |State(state): State<FakeState>, headers: HeaderMap, body: Bytes| async move {
                    let mut refuse = state.refuse.lock().unwrap();
                    if *refuse > 0 {
                        *refuse -= 1;
                        return StatusCode::NOT_FOUND;
                    }
                    let content_type = headers
                        .get(header::CONTENT_TYPE)
                        .and_then(|value| value.to_str().ok())
                        .map(String::from);
                    state
                        .launches
                        .lock()
                        .unwrap()
                        .push((content_type, String::from_utf8_lossy(&body).into()));
                    StatusCode::CREATED
                },
            ),
        )
        .with_state(state);
    tokio::spawn(async move { axum::serve(listener, router).await });

    let responder = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let search = responder.local_addr().unwrap();
    let location = base.clone();
    let answering = running.clone();
    tokio::spawn(async move {
        let mut buffer = [0u8; 1024];
        while let Ok((_, peer)) = responder.recv_from(&mut buffer).await {
            if !answering.load(Ordering::Relaxed) {
                continue;
            }
            let answer = format!(
                "HTTP/1.1 200 OK\r\nLOCATION: {location}/\r\n\
                 SERVER: linux/7.3 UPnP/1.0 VacuumTube/2.0.1\r\nST: {DIAL_TARGET}\r\n\r\n"
            );
            let _ = responder.send_to(answer.as_bytes(), peer).await;
        }
    });
    FakeVacuumTube {
        search,
        base,
        launches,
        running,
    }
}

/// The address of a search that nobody answers: VacuumTube is closed
async fn silent_search_address() -> SocketAddr {
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let address = socket.local_addr().unwrap();
    // The socket stays open for the test, so the search does not fail.
    std::mem::forget(socket);
    address
}

fn test_home(name: &str) -> PathBuf {
    let home =
        std::env::temp_dir().join(format!("htpc-control-dial-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    home
}

/// A start of VacuumTube that does nothing
fn no_start() -> Starter {
    Arc::new(|| Box::pin(async { Ok(()) }))
}

fn test_dial(home: &std::path::Path, search: SocketAddr) -> Arc<Dial> {
    let controller = Controller::new(Config::for_test(home.to_path_buf()));
    Dial::with_search_address(controller, 8780, search, no_start())
}

/// Serves the DIAL routes on a free local port, and returns the base
/// address of the server.
async fn serve(dial: &Arc<Dial>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let router = dial.router();
    tokio::spawn(async move { axum::serve(listener, router).await });
    format!("http://{address}")
}

#[tokio::test]
async fn the_description_has_the_identity_and_the_address_of_the_apps() {
    let home = test_home("description");
    let dial = test_dial(&home, silent_search_address().await);
    let server = serve(&dial).await;
    let answer = reqwest::get(format!("{server}/dial/device.xml"))
        .await
        .unwrap();
    assert_eq!(
        answer.headers()["application-url"],
        format!("{server}/dial/apps/")
    );
    let text = answer.text().await.unwrap();
    assert_eq!(
        element_text(&text, "friendlyName").as_deref(),
        Some("Test HTPC (VacuumTube)")
    );
    assert!(element_text(&text, "UDN").unwrap().starts_with("uuid:"));
    let _ = std::fs::remove_dir_all(home);
}

#[tokio::test]
async fn the_app_is_stopped_while_vacuumtube_is_closed() {
    let home = test_home("stopped");
    let dial = test_dial(&home, silent_search_address().await);
    let server = serve(&dial).await;
    let text = reqwest::get(format!("{server}/dial/apps/YouTube"))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(element_text(&text, "state").as_deref(), Some("stopped"));
    let _ = std::fs::remove_dir_all(home);
}

#[tokio::test]
async fn the_app_name_has_no_case_and_other_apps_do_not_exist() {
    let home = test_home("names");
    let dial = test_dial(&home, silent_search_address().await);
    let server = serve(&dial).await;
    for name in ["YouTube", "youtube", "YOUTUBE"] {
        let status = reqwest::get(format!("{server}/dial/apps/{name}"))
            .await
            .unwrap()
            .status();
        assert_eq!(status, StatusCode::OK, "{name}");
    }
    let status = reqwest::get(format!("{server}/dial/apps/Netflix"))
        .await
        .unwrap()
        .status();
    assert_eq!(status, StatusCode::NOT_FOUND);
    let status = reqwest::Client::new()
        .post(format!("{server}/dial/apps/Netflix"))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(status, StatusCode::NOT_FOUND);
    let _ = std::fs::remove_dir_all(home);
}

#[tokio::test]
async fn the_state_comes_from_vacuumtube_with_the_address_of_this_service() {
    let home = test_home("running");
    let fake = fake_vacuumtube(0).await;
    let dial = test_dial(&home, fake.search);
    let server = serve(&dial).await;
    let text = reqwest::get(format!("{server}/dial/apps/YouTube"))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(element_text(&text, "state").as_deref(), Some("running"));
    // A phone must not learn the port of VacuumTube. It changes at each start.
    assert!(
        text.contains(&format!("{server}/dial/apps/YouTube/run")),
        "{text}"
    );
    assert!(!text.contains(&fake.base), "{text}");
    let _ = std::fs::remove_dir_all(home);
}

#[tokio::test]
async fn a_launch_goes_on_to_the_running_vacuumtube_unchanged() {
    let home = test_home("forward");
    let fake = fake_vacuumtube(0).await;
    let dial = test_dial(&home, fake.search);
    let server = serve(&dial).await;
    let answer = reqwest::Client::new()
        .post(format!("{server}/dial/apps/YouTube"))
        .header("Content-Type", "text/plain; charset=utf-8")
        .body("pairingCode=1234&theme=cl&v=abc")
        .send()
        .await
        .unwrap();
    assert_eq!(answer.status(), StatusCode::CREATED);
    assert_eq!(
        answer.headers()["location"],
        format!("{server}/dial/apps/YouTube/run")
    );
    let launches = fake.launches.lock().unwrap().clone();
    assert_eq!(
        launches,
        vec![(
            Some("text/plain; charset=utf-8".to_string()),
            "pairingCode=1234&theme=cl&v=abc".to_string()
        )]
    );
    let _ = std::fs::remove_dir_all(home);
}

#[tokio::test]
async fn the_launch_waits_until_the_app_page_of_vacuumtube_is_ready() {
    let home = test_home("retry");
    let fake = fake_vacuumtube(2).await;
    let dial = test_dial(&home, fake.search);
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain"));
    let status = dial
        .forward_until_taken(&headers, Bytes::from("pairingCode=1"))
        .await;
    assert_eq!(status, Some(StatusCode::CREATED));
    // The two refused requests are not in the record.
    assert_eq!(fake.launches.lock().unwrap().len(), 1);
    let _ = std::fs::remove_dir_all(home);
}

#[tokio::test]
async fn the_service_learns_and_keeps_the_identity_of_vacuumtube() {
    let home = test_home("identity");
    let fake = fake_vacuumtube(0).await;
    let dial = test_dial(&home, fake.search);
    assert_ne!(dial.identity().udn, FAKE_UDN);
    assert!(dial.find_vacuumtube().await.is_some());
    let learned = dial.identity();
    assert_eq!(
        (learned.name.as_str(), learned.udn.as_str()),
        (FAKE_NAME, FAKE_UDN)
    );
    // The next start of the service reads the saved file.
    let file = std::fs::read_to_string(home.join(".local/state/x86-linux-htpc/dial-identity.json"))
        .unwrap();
    let saved: serde_json::Value = serde_json::from_str(&file).unwrap();
    assert_eq!(saved["udn"], FAKE_UDN);
    assert_eq!(saved["name"], FAKE_NAME);
    let restarted = test_dial(&home, silent_search_address().await);
    assert_eq!(restarted.identity(), learned);
    let _ = std::fs::remove_dir_all(home);
}

#[tokio::test]
async fn the_service_learns_the_identity_when_vacuumtube_starts() {
    let home = test_home("watcher");
    let fake = fake_vacuumtube(0).await;
    let dial = test_dial(&home, fake.search);
    tokio::spawn(dial.clone().watch_vacuumtube());
    // VacuumTube is closed: the service keeps its own identity.
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_ne!(dial.identity().udn, FAKE_UDN);
    dial.controller.publish_status(Status {
        running: Running {
            youtube: true,
            kodi: false,
        },
        ..Status::default()
    });
    for _ in 0..50 {
        if dial.identity().udn == FAKE_UDN {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(dial.identity().udn, FAKE_UDN);
    let _ = std::fs::remove_dir_all(home);
}

/// A phone on the local network: it sends searches and reads the answer.
struct FakePhone {
    socket: UdpSocket,
    ssdp: SocketAddr,
}

impl FakePhone {
    async fn new(ssdp: SocketAddr) -> FakePhone {
        FakePhone {
            socket: UdpSocket::bind("127.0.0.1:0").await.unwrap(),
            ssdp,
        }
    }

    /// The answer of the service to a DIAL search, or None if it stays silent
    async fn search(&self, wait: Duration) -> Option<String> {
        let search = format!(
            "M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\n\
             MX: 1\r\nST: {DIAL_TARGET}\r\nUSER-AGENT: Phone/1.0\r\n\r\n"
        );
        self.socket
            .send_to(search.as_bytes(), self.ssdp)
            .await
            .unwrap();
        let mut buffer = [0u8; 2048];
        let (length, _) = tokio::time::timeout(wait, self.socket.recv_from(&mut buffer))
            .await
            .ok()?
            .ok()?;
        Some(String::from_utf8_lossy(&buffer[..length]).into())
    }
}

fn header_of(answer: &str, name: &str) -> Option<String> {
    answer.split("\r\n").find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.eq_ignore_ascii_case(name)
            .then(|| value.trim().to_string())
    })
}

/// The whole way of a cast: the phone finds the HTPC, VacuumTube is
/// closed, the phone starts it, the data of the phone arrives at
/// VacuumTube, and the HTPC keeps one identity before and after.
#[tokio::test]
async fn a_phone_starts_youtube_on_the_closed_htpc() {
    let home = test_home("workflow");
    // VacuumTube is closed, and it refuses the first two launches after
    // its start, as the real one does until its app page is ready.
    let fake = fake_vacuumtube_with(2, false).await;
    let running = fake.running.clone();
    let starter: Starter = Arc::new(move || {
        let running = running.clone();
        Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            running.store(true, Ordering::Relaxed);
            Ok(())
        })
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let controller = Controller::new(Config::for_test(home.clone()));
    let dial = Dial::with_search_address(controller, port, fake.search, starter);
    let router = dial.router();
    tokio::spawn(async move { axum::serve(listener, router).await });
    let ssdp = DialSsdp::bind_to(dial.clone(), "127.0.0.1:0".parse().unwrap(), false).unwrap();
    let ssdp_address = ssdp.socket.local_addr().unwrap();
    tokio::spawn(Arc::new(ssdp).run());
    let phone = FakePhone::new(ssdp_address).await;
    let http = reqwest::Client::new();
    let server = format!("http://127.0.0.1:{port}");

    // 1. The phone searches and gets the answer of the service.
    let answer = phone
        .search(Duration::from_secs(3))
        .await
        .expect("an answer");
    assert_eq!(
        header_of(&answer, "LOCATION"),
        Some(format!("{server}/dial/device.xml"))
    );
    let usn = header_of(&answer, "USN").unwrap();
    let own_udn = usn.split("::").next().unwrap().to_string();

    // 2. It reads the description, and the name of the app is stopped.
    let description = http
        .get(format!("{server}/dial/device.xml"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        description.headers()["application-url"],
        format!("{server}/dial/apps/")
    );
    let text = description.text().await.unwrap();
    assert_eq!(
        element_text(&text, "UDN").as_deref(),
        Some(own_udn.as_str())
    );
    let state = http
        .get(format!("{server}/dial/apps/YouTube"))
        .header("Origin", "package:com.google.android.youtube")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(element_text(&state, "state").as_deref(), Some("stopped"));

    // 3. The phone starts the app. The answer comes at once, before
    // VacuumTube is up.
    let started = std::time::Instant::now();
    let launch = http
        .post(format!("{server}/dial/apps/YouTube"))
        .header("Content-Type", "text/plain; charset=utf-8")
        .body("pairingCode=abcd-1234&theme=cl")
        .send()
        .await
        .unwrap();
    assert_eq!(launch.status(), StatusCode::CREATED);
    // The answer includes the search for a running VacuumTube, but it
    // does not wait for the start.
    assert!(
        started.elapsed() < Duration::from_millis(1800),
        "the answer waited for VacuumTube"
    );
    assert!(!fake.running.load(Ordering::Relaxed));

    // 4. The next state request waits for VacuumTube, and the answer is
    // "running". A "stopped" now would make the phone give up.
    let state = http
        .get(format!("{server}/dial/apps/YouTube"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(
        element_text(&state, "state").as_deref(),
        Some("running"),
        "{state}"
    );
    assert!(!state.contains(&fake.base));

    // 5. VacuumTube takes the data of the phone, without a change.
    for _ in 0..100 {
        if !fake.launches.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(
        fake.launches.lock().unwrap().clone(),
        vec![(
            Some("text/plain; charset=utf-8".to_string()),
            "pairingCode=abcd-1234&theme=cl".to_string()
        )]
    );

    // 6. While VacuumTube runs, it answers the searches, and the service
    // stays silent. The phone sees one device.
    assert_eq!(phone.search(Duration::from_millis(1500)).await, None);

    // 7. The service has taken the identity of VacuumTube. After
    // VacuumTube closes, the service answers with it.
    assert_eq!(dial.identity().udn, FAKE_UDN);
    fake.running.store(false, Ordering::Relaxed);
    let answer = phone
        .search(Duration::from_secs(3))
        .await
        .expect("an answer");
    assert_eq!(
        header_of(&answer, "USN"),
        Some(format!("{FAKE_UDN}::{DIAL_TARGET}"))
    );
    let text = http
        .get(format!("{server}/dial/device.xml"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(element_text(&text, "UDN").as_deref(), Some(FAKE_UDN));
    assert_eq!(
        element_text(&text, "friendlyName").as_deref(),
        Some(FAKE_NAME)
    );
    let _ = std::fs::remove_dir_all(home);
}
