//! The local socket that serves commands to a browser during development.
//!
//! It listens on loopback only, on a port the OS picks, and refuses any connection whose
//! upgrade request does not carry the token as a bearer credential. A web page cannot set
//! that header on a WebSocket, so a page in the user's browser cannot reach the core.

use std::io;
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

use crate::Api;
use crate::command::{Request, Response};
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

async fn serve_connection(
    stream: TcpStream,
    api: Arc<Api>,
    expected: Arc<str>,
    telemetry: watch::Receiver<Option<TelemetryFrame>>,
) {
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
    let writer = tokio::spawn(async move {
        let mut telemetry = telemetry;
        let mut sent_waveforms = [0u32; 2];
        loop {
            let message = tokio::select! {
                reply = to_send.recv() => match reply {
                    Some(reply) => Message::text(reply),
                    None => break,
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
    while let Some(Ok(message)) = incoming.next().await {
        let Message::Text(text) = message else {
            continue;
        };
        let reply = serde_json::to_string(&answer(Arc::clone(&api), text.as_str()).await);
        let Ok(reply) = reply else {
            break;
        };
        if replies.send(reply).await.is_err() {
            break;
        }
    }
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
