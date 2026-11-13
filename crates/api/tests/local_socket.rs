#![allow(
    clippy::unwrap_used,
    reason = "every function in this file is test code"
)]

use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use resonance_api::Api;
use resonance_api::serve::Server;
use resonance_db::Database;
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
    let reply = socket.next().await.unwrap().unwrap();
    serde_json::from_str(reply.to_text().unwrap()).unwrap()
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
