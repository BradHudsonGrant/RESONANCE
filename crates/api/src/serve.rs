//! The local socket that serves commands to a browser during development.
//!
//! It listens on loopback only, on a port the OS picks, and refuses any connection whose
//! upgrade request does not carry the token as a bearer credential. A web page cannot set
//! that header on a WebSocket, so a page in the user's browser cannot reach the core.

use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::accept_hdr_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{
    ErrorResponse, Request as Upgrade, Response as Accepted,
};
use tokio_tungstenite::tungstenite::http::{StatusCode, header::AUTHORIZATION};
use ts_rs::TS;

use crate::Api;
use crate::command::{Request, Response};
use crate::error::ApiError;

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
        loop {
            let (stream, _) = self.listener.accept().await?;
            tokio::spawn(serve_connection(
                stream,
                Arc::clone(&api),
                Arc::clone(&expected),
            ));
        }
    }
}

async fn serve_connection(stream: TcpStream, api: Arc<Api>, expected: Arc<str>) {
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
    let Ok(mut socket) = accept_hdr_async(stream, authorise).await else {
        return;
    };
    while let Some(Ok(message)) = socket.next().await {
        let Message::Text(text) = message else {
            continue;
        };
        let reply = serde_json::to_string(&answer(&api, text.as_str()));
        let Ok(reply) = reply else {
            break;
        };
        if socket.send(Message::text(reply)).await.is_err() {
            break;
        }
    }
}

fn answer(api: &Api, text: &str) -> Reply {
    match serde_json::from_str::<Call>(text) {
        Ok(call) => match api.handle(call.request) {
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
        },
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
