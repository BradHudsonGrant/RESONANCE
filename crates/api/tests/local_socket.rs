#![allow(
    clippy::unwrap_used,
    reason = "every function in this file is test code"
)]

use std::sync::Arc;
use std::{fs, path::Path, time::SystemTime};

use futures_util::{SinkExt, StreamExt};
use resonance_api::Api;
use resonance_api::serve::Server;
use resonance_db::Database;
use resonance_fixtures::signal::Signal;
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::{HeaderValue, StatusCode, header::AUTHORIZATION};
use tokio_tungstenite::tungstenite::{Error, Message};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

struct Running {
    url: String,
    token: String,
}

async fn start() -> Running {
    let server = Server::bind().await.unwrap();
    let running = Running {
        url: format!("ws://{}", server.address().unwrap()),
        token: server.token().to_owned(),
    };
    tokio::spawn(server.run(Arc::new(Api::new(Database::open_in_memory().unwrap()))));
    running
}

async fn connect(url: &str, bearer: Option<&str>) -> Result<Socket, Error> {
    let mut request = url.into_client_request().unwrap();
    if let Some(bearer) = bearer {
        let value = HeaderValue::from_str(&format!("Bearer {bearer}")).unwrap();
        request.headers_mut().insert(AUTHORIZATION, value);
    }
    connect_async(request).await.map(|(socket, _)| socket)
}

async fn ask(socket: &mut Socket, text: &str) -> Value {
    socket.send(Message::text(text)).await.unwrap();
    loop {
        let reply = socket.next().await.unwrap().unwrap();
        let parsed: Value = serde_json::from_str(reply.to_text().unwrap()).unwrap();
        // Unsolicited push notifications (`library_changed`, `activity_changed`) carry neither a
        // call id nor an error.
        if parsed["id"].is_null() && parsed["error"].is_null() {
            continue;
        }
        return parsed;
    }
}

fn refused_as_unauthorised(result: Result<Socket, Error>) -> bool {
    matches!(result, Err(Error::Http(response)) if response.status() == StatusCode::UNAUTHORIZED)
}

#[tokio::test]
async fn a_call_with_the_token_gets_its_answer() {
    let core = start().await;
    let mut socket = connect(&core.url, Some(&core.token)).await.unwrap();

    let reply = ask(
        &mut socket,
        r#"{"id": 7, "request": {"command": "app_info"}}"#,
    )
    .await;

    assert_eq!(
        reply,
        json!({
            "id": 7,
            "response": {"command": "app_info", "result": {"version": env!("CARGO_PKG_VERSION")}},
            "error": null
        })
    );
}

#[tokio::test]
async fn a_connection_without_the_token_is_refused() {
    let core = start().await;

    assert!(refused_as_unauthorised(connect(&core.url, None).await));
}

#[tokio::test]
async fn a_connection_with_the_wrong_token_is_refused() {
    let core = start().await;

    assert!(refused_as_unauthorised(
        connect(&core.url, Some("not-the-token")).await
    ));
}

#[tokio::test]
async fn an_unknown_command_is_answered_with_an_error_that_keeps_the_call_id() {
    let core = start().await;
    let mut socket = connect(&core.url, Some(&core.token)).await.unwrap();

    let reply = ask(
        &mut socket,
        r#"{"id": 3, "request": {"command": "no_such_command"}}"#,
    )
    .await;

    assert_eq!(reply["id"], 3);
    assert_eq!(reply["response"], Value::Null);
    assert_eq!(reply["error"]["code"], "bad_request");
}

#[tokio::test]
async fn text_that_is_not_a_call_is_answered_with_an_error_and_the_socket_stays_open() {
    let core = start().await;
    let mut socket = connect(&core.url, Some(&core.token)).await.unwrap();

    let reply = ask(&mut socket, "not json").await;
    assert_eq!(reply["id"], Value::Null);
    assert_eq!(reply["error"]["code"], "bad_request");

    let reply = ask(
        &mut socket,
        r#"{"id": 1, "request": {"command": "app_info"}}"#,
    )
    .await;
    assert_eq!(reply["id"], 1);
    assert_eq!(reply["error"], Value::Null);
}

#[tokio::test]
async fn each_server_makes_its_own_token() {
    let (first, second) = (start().await, start().await);

    assert_eq!(first.token.len(), 64);
    assert_ne!(first.token, second.token);
}

#[tokio::test]
async fn a_scan_over_the_socket_makes_its_tracks_available_on_the_same_connection() {
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("socket-scan")
        .join(format!("{}-{unique}", std::process::id()));
    fs::create_dir_all(&folder).unwrap();
    Signal::tone(440.0, 0.1, -6.0, 48_000)
        .write_wav(&folder.join("First Light.wav"))
        .unwrap();
    let core = start().await;
    let mut socket = connect(&core.url, Some(&core.token)).await.unwrap();

    let report = ask(
        &mut socket,
        &json!({
            "id": 1,
            "request": {"command": "scan", "params": {"folder": folder}}
        })
        .to_string(),
    )
    .await;
    let page = ask(
        &mut socket,
        r#"{"id": 2, "request": {"command": "tracks", "params": {"offset": 0, "limit": 200}}}"#,
    )
    .await;

    assert_eq!(report["id"], 1);
    assert_eq!(report["error"], Value::Null);
    assert_eq!(report["response"]["result"]["added"], 1);
    assert_eq!(page["id"], 2);
    assert_eq!(page["error"], Value::Null);
    assert_eq!(page["response"]["result"]["total"], 1);
    assert_eq!(
        page["response"]["result"]["tracks"][0]["title"],
        "First Light"
    );
}

fn get(address: &str, target: &str, bearer: Option<&str>) -> (String, Vec<u8>) {
    use std::io::{Read, Write};
    let mut stream = std::net::TcpStream::connect(address).unwrap();
    let authorization = bearer
        .map(|token| format!("Authorization: Bearer {token}\r\n"))
        .unwrap_or_default();
    write!(
        stream,
        "GET {target} HTTP/1.1\r\nHost: {address}\r\n{authorization}\r\n"
    )
    .unwrap();
    let mut answer = Vec::new();
    stream.read_to_end(&mut answer).unwrap();
    let split = answer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .unwrap();
    let head = String::from_utf8(answer[..split].to_vec()).unwrap();
    (head, answer[split + 4..].to_vec())
}

#[tokio::test]
async fn a_picture_set_over_the_socket_is_served_by_hash_with_the_token() {
    let folder = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("socket-artwork");
    let _ = fs::remove_dir_all(&folder);
    let mut database = Database::open_in_memory().unwrap();
    database.set_artwork_folder(&folder);
    let source = database.source("C:/Music").unwrap();
    database
        .record_scan(
            source,
            &[resonance_core::library::ScannedFile {
                path: "One.mp3".to_owned(),
                size_bytes: 1,
                modified_ns: 1,
                format: resonance_core::library::AudioFormat::Mp3,
                audio: None,
                tags: Vec::new(),
            }],
        )
        .unwrap();
    let track_id = database.tracks().unwrap()[0].track_id;
    let server = Server::bind().await.unwrap();
    let address = server.address().unwrap().to_string();
    let token = server.token().to_owned();
    let url = format!("ws://{address}");
    tokio::spawn(server.run(Arc::new(Api::new(database))));
    let mut socket = connect(&url, Some(&token)).await.unwrap();

    // "/9j/4AAQ" is the start of a JPEG.
    let reply = ask(
        &mut socket,
        &json!({ "id": 1, "request": { "command": "set_track_artwork",
                 "params": { "track_id": track_id, "image_base64": "/9j/4AAQ" } } })
        .to_string(),
    )
    .await;
    let hash = reply["response"]["result"]["artwork"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(hash.len(), 64);

    let target = format!("/artwork/{hash}");
    let (served, refused, unknown) = tokio::task::spawn_blocking(move || {
        (
            get(&address, &target, Some(&token)),
            get(&address, &target, None),
            get(&address, "/artwork/../library.db", Some(&token)),
        )
    })
    .await
    .unwrap();

    assert!(served.0.starts_with("HTTP/1.1 200 OK"), "{}", served.0);
    assert!(served.0.contains("Content-Type: image/jpeg"));
    assert!(served.0.contains("immutable"));
    assert_eq!(served.1, [0xff, 0xd8, 0xff, 0xe0, 0, 16]);
    assert!(refused.0.starts_with("HTTP/1.1 401"), "{}", refused.0);
    assert!(refused.1.is_empty());
    assert!(unknown.0.starts_with("HTTP/1.1 404"), "{}", unknown.0);

    let refused_picture = ask(
        &mut socket,
        &json!({ "id": 2, "request": { "command": "set_track_artwork",
                 "params": { "track_id": track_id, "image_base64": "R0lGODlh" } } })
        .to_string(),
    )
    .await;
    assert_eq!(
        refused_picture["error"]["message"],
        "The picture is not a JPEG or PNG"
    );
}

#[tokio::test]
async fn overviews_are_served_for_the_tracks_asked_for_in_their_order() {
    let folder = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("socket-overviews");
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).unwrap();
    Signal::tone(700.0, 2.0, -6.0, 44_100)
        .write_flac(&folder.join("tone.flac"))
        .unwrap();
    let mut database = Database::open_in_memory().unwrap();
    resonance_library::scan::scan(&mut database, &folder, &[]).unwrap();
    resonance_library::identity::identify_pending(&mut database).unwrap();
    let track_id = database.tracks().unwrap()[0].track_id;
    let server = Server::bind().await.unwrap();
    let address = server.address().unwrap().to_string();
    let token = server.token().to_owned();
    tokio::spawn(server.run(Arc::new(Api::new(database))));

    let target = format!("/waveforms?tracks={},{track_id}", track_id + 1);
    let (served, refused, malformed) = tokio::task::spawn_blocking(move || {
        (
            get(&address, &target, Some(&token)),
            get(&address, &target, None),
            get(&address, "/waveforms?tracks=one", Some(&token)),
        )
    })
    .await
    .unwrap();

    assert!(served.0.starts_with("HTTP/1.1 200 OK"), "{}", served.0);
    assert!(served.0.contains("Content-Type: application/octet-stream"));
    assert!(served.0.contains("no-store"));
    let body = served.1;
    let entry = 1 + 140 * 4;
    assert_eq!(body[0], 3);
    assert_eq!(body[4..12], [140, 0, 0, 0, 2, 0, 0, 0]);
    assert_eq!(body.len(), 12 + 2 * entry);
    assert_eq!(body[12], 0);
    let shown = &body[12 + entry..];
    assert_eq!(shown[0], 1);
    // The first column's peak: a -6 dBFS tone peaks at half scale.
    assert_eq!(shown[1], 128);
    assert!(refused.0.starts_with("HTTP/1.1 401"), "{}", refused.0);
    assert!(malformed.0.starts_with("HTTP/1.1 400"), "{}", malformed.0);
}
