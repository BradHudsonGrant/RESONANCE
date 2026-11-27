//! The local socket that serves commands to a browser during development.
//!
//! It listens on loopback only, on a port the OS picks, and refuses any connection whose
//! upgrade request does not carry the token as a bearer credential. A web page cannot set
//! that header on a WebSocket, so a page in the user's browser cannot reach the core.
//!
//! The same port answers `GET /artwork/<hash>` with a stored picture and `GET /waveforms?tracks=`
//! with waveform overviews, under the same token, so the page loads pictures as plain images and
//! overviews as binary rather than inside the JSON of a track page.

use std::io::{self, BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use resonance_engine::DeckId;
use serde::{Deserialize, Serialize};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::accept_hdr_async;
use tokio_tungstenite::tungstenite::handshake::server::{
    ErrorResponse, Request as Upgrade, Response as Accepted,
};
use tokio_tungstenite::tungstenite::http::{StatusCode, header::AUTHORIZATION};
use tokio_tungstenite::tungstenite::{Bytes, Message};
use ts_rs::TS;

use crate::{Api, overviews};
use crate::command::{ActivityChanged, LibraryChanged, Request, Response};
use crate::error::ApiError;
use crate::mix_frame;

/// How often the decks are read for the page, within the 60 Hz the architecture allows. The page
/// moves the playheads on between readings.
const TELEMETRY_INTERVAL: Duration = Duration::from_micros(16_667);

/// One reading of the decks, encoded once and shared by every connection.
#[derive(Debug, Clone)]
struct TelemetryFrame {
    bytes: Bytes,
    generations: [u32; 2],
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct Call {
    pub id: u32,
    pub request: Request,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct Reply {
    pub id: Option<u32>,
    pub response: Option<Response>,
    pub error: Option<ApiError>,
}

pub struct Server {
    listener: TcpListener,
    token: String,
}

impl Server {
    pub async fn bind() -> io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(io::Error::other)?;
        let token = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        Ok(Self { listener, token })
    }

    pub fn address(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    pub async fn run(self, api: Arc<Api>) -> io::Result<()> {
        let expected: Arc<str> = format!("Bearer {}", self.token).into();
        let (readings, telemetry) = watch::channel(None);
        let sampled = Arc::clone(&api);
        thread::Builder::new()
            .name("mix telemetry".to_owned())
            .spawn(move || sample_telemetry(&sampled, &readings))?;
        loop {
            let (stream, _) = self.listener.accept().await?;
            tokio::spawn(serve_connection(
                stream,
                Arc::clone(&api),
                Arc::clone(&expected),
                telemetry.clone(),
            ));
        }
    }
}

/// Reads the decks on a steady beat while a page is connected and the device is open. It runs
/// on its own thread because a load holds the mix while the device opens.
fn sample_telemetry(api: &Api, readings: &watch::Sender<Option<TelemetryFrame>>) {
    let mut next = Instant::now();
    while !readings.is_closed() {
        next += TELEMETRY_INTERVAL;
        // The server keeps one receiver to hand out; any more are pages.
        if readings.receiver_count() > 1
            && let Some(telemetry) = api.mix().telemetry()
        {
            readings.send_replace(Some(TelemetryFrame {
                bytes: mix_frame::telemetry(&telemetry).into(),
                generations: telemetry.decks.map(|deck| deck.generation),
            }));
        }
        let now = Instant::now();
        if next > now {
            thread::sleep(next - now);
        } else {
            next = now;
        }
    }
}

const ARTWORK_REQUEST: &str = "/artwork/";
const OVERVIEWS_REQUEST: &str = "/waveforms?";
/// The starts of the plain requests the port answers, as they arrive on the wire.
const PLAIN_REQUESTS: [&[u8]; 2] = [b"GET /artwork/", b"GET /waveforms?"];
/// More than any browser sends for an image request.
const MAX_REQUEST_HEAD: u64 = 16 * 1024;

/// Whether the connection opens with a picture or overview request rather than a WebSocket
/// upgrade.
async fn asks_for_plain_request(stream: &TcpStream) -> bool {
    let mut start = [0u8; 15];
    for _ in 0..8 {
        let Ok(read) = stream.peek(&mut start).await else {
            return false;
        };
        let arrived = &start[..read];
        let mut could_be = false;
        for request in PLAIN_REQUESTS {
            let length = read.min(request.len());
            if arrived[..length] == request[..length] {
                if read >= request.len() {
                    return true;
                }
                could_be = true;
            }
        }
        if read == 0 || !could_be {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    false
}

fn respond(stream: &mut std::net::TcpStream, status: &str, extra: &str, body: &[u8]) {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\n{extra}Connection: close\r\n\r\n",
        body.len()
    );
    let _ = stream
        .write_all(head.as_bytes())
        .and_then(|()| stream.write_all(body));
}

/// Answers one picture or overview request and closes.
fn serve_plain_request(stream: std::net::TcpStream, api: &Api, expected: &str) {
    let Ok(mut writer) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(io::Read::take(stream, MAX_REQUEST_HEAD));
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    let mut authorised = false;
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) if line.trim_end_matches(['\r', '\n']).is_empty() => break,
            Ok(_) => {
                if let Some((name, value)) = line.split_once(':')
                    && name.trim().eq_ignore_ascii_case(AUTHORIZATION.as_str())
                {
                    authorised = same_bytes(value.trim().as_bytes(), expected.as_bytes());
                }
            }
        }
    }
    if !authorised {
        respond(&mut writer, "401 Unauthorized", "", b"");
        return;
    }
    let target = request_line.split_whitespace().nth(1).unwrap_or_default();
    if let Some(query) = target.strip_prefix(OVERVIEWS_REQUEST) {
        serve_overviews(&mut writer, api, query);
        return;
    }
    let hash = target.strip_prefix(ARTWORK_REQUEST).unwrap_or_default();
    serve_artwork(&mut writer, api, hash);
}

/// A picture never changes under its hash, so the browser may keep it as long as it likes.
fn serve_artwork(writer: &mut std::net::TcpStream, api: &Api, hash: &str) {
    let Some(file) = api.artwork_file(hash) else {
        respond(writer, "404 Not Found", "", b"");
        return;
    };
    match std::fs::read(&file.path) {
        Ok(bytes) => respond(
            writer,
            "200 OK",
            &format!(
                "Content-Type: {}\r\nCache-Control: private, max-age=31536000, immutable\r\n",
                file.mime
            ),
            &bytes,
        ),
        Err(_) => respond(writer, "404 Not Found", "", b""),
    }
}

/// Overviews change when a file is read again, so the browser keeps none of them.
fn serve_overviews(writer: &mut std::net::TcpStream, api: &Api, query: &str) {
    let Some(track_ids) = overviews::track_ids(query) else {
        respond(writer, "400 Bad Request", "", b"");
        return;
    };
    match api.waveform_overviews(&track_ids) {
        Ok(body) => respond(
            writer,
            "200 OK",
            "Content-Type: application/octet-stream\r\nCache-Control: no-store\r\n",
            &body,
        ),
        Err(_) => respond(writer, "500 Internal Server Error", "", b""),
    }
}

async fn serve_connection(
    stream: TcpStream,
    api: Arc<Api>,
    expected: Arc<str>,
    telemetry: watch::Receiver<Option<TelemetryFrame>>,
) {
    if asks_for_plain_request(&stream).await {
        let Ok(stream) = stream.into_std() else {
            return;
        };
        // A client that stops sending must not hold a blocking thread.
        if stream.set_nonblocking(false).is_err()
            || stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .is_err()
        {
            return;
        }
        let _ =
            tokio::task::spawn_blocking(move || serve_plain_request(stream, &api, &expected)).await;
        return;
    }
    #[expect(
        clippy::result_large_err,
        reason = "tungstenite fixes the signature of its handshake callback"
    )]
    let authorise = |upgrade: &Upgrade, accepted: Accepted| {
        let presented = upgrade
            .headers()
            .get(AUTHORIZATION)
            .map(|value| value.as_bytes());
        if presented.is_some_and(|presented| same_bytes(presented, expected.as_bytes())) {
            Ok(accepted)
        } else {
            let mut refused = ErrorResponse::new(None);
            *refused.status_mut() = StatusCode::UNAUTHORIZED;
            Err(refused)
        }
    };
    let Ok(socket) = accept_hdr_async(stream, authorise).await else {
        return;
    };
    let (mut outgoing, mut incoming) = socket.split();
    let (replies, mut to_send) = mpsc::channel::<String>(16);
    let writer_api = Arc::clone(&api);
    let mut watch_revisions = api.watch_revisions();
    let mut activity_revisions = api.activity_revisions();
    let writer = tokio::spawn(async move {
        let mut telemetry = telemetry;
        let mut sent_waveforms = [0u32; 2];
        loop {
            let message = tokio::select! {
                reply = to_send.recv() => match reply {
                    Some(reply) => Message::text(reply),
                    None => break,
                },
                changed = watch_revisions.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    let revision = *watch_revisions.borrow_and_update();
                    let Ok(push) = serde_json::to_string(&Reply {
                        id: None,
                        response: Some(Response::LibraryChanged(LibraryChanged { revision })),
                        error: None,
                    }) else {
                        break;
                    };
                    Message::text(push)
                },
                changed = activity_revisions.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    let revision = *activity_revisions.borrow_and_update();
                    let Ok(push) = serde_json::to_string(&Reply {
                        id: None,
                        response: Some(Response::ActivityChanged(ActivityChanged { revision })),
                        error: None,
                    }) else {
                        break;
                    };
                    Message::text(push)
                },
                changed = telemetry.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    let Some(frame) = telemetry.borrow_and_update().clone() else {
                        continue;
                    };
                    for deck in DeckId::ALL {
                        let generation = frame.generations[deck.index()];
                        if generation == sent_waveforms[deck.index()] {
                            continue;
                        }
                        let Some((ready, waveform)) = writer_api.mix().waveform(deck) else {
                            continue;
                        };
                        let bytes = mix_frame::waveform(deck, ready, &waveform);
                        if outgoing.send(Message::binary(bytes)).await.is_err() {
                            return;
                        }
                        sent_waveforms[deck.index()] = ready;
                    }
                    Message::binary(frame.bytes)
                }
            };
            if outgoing.send(message).await.is_err() {
                break;
            }
        }
    });
    let mut in_flight = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            next_message = incoming.next() => {
                let Some(Ok(message)) = next_message else {
                    break;
                };
                let Message::Text(text) = message else {
                    continue;
                };
                let call_api = Arc::clone(&api);
                let call_replies = replies.clone();
                in_flight.spawn(async move {
                    let reply = answer(call_api, text.as_str()).await;
                    if let Ok(encoded) = serde_json::to_string(&reply) {
                        let _ = call_replies.send(encoded).await;
                    }
                });
            }
            Some(_) = in_flight.join_next(), if !in_flight.is_empty() => {}
        }
    }
    in_flight.abort_all();
    while in_flight.join_next().await.is_some() {}
    drop(replies);
    let _ = writer.await;
}

async fn answer(api: Arc<Api>, text: &str) -> Reply {
    match serde_json::from_str::<Call>(text) {
        Ok(call) => {
            let result = tokio::task::spawn_blocking(move || api.handle(call.request))
                .await
                .map_err(ApiError::internal)
                .and_then(|result| result);
            match result {
                Ok(response) => Reply {
                    id: Some(call.id),
                    response: Some(response),
                    error: None,
                },
                Err(error) => Reply {
                    id: Some(call.id),
                    response: None,
                    error: Some(error),
                },
            }
        }
        Err(error) => Reply {
            id: id_of(text),
            response: None,
            error: Some(ApiError::bad_request(error)),
        },
    }
}

/// Recovers the id from a call that did not parse, so the caller can still match the error to it.
fn id_of(text: &str) -> Option<u32> {
    #[derive(Deserialize)]
    struct OnlyId {
        id: u32,
    }
    serde_json::from_str::<OnlyId>(text)
        .ok()
        .map(|call| call.id)
}

/// Compares without stopping at the first difference, so timing says nothing about the token.
fn same_bytes(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .fold(0u8, |difference, (a, b)| difference | (a ^ b))
            == 0
}
