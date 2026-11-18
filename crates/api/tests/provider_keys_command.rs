#![allow(
    clippy::unwrap_used,
    reason = "every function in this file is test code"
)]

use std::sync::Arc;

use resonance_api::Api;
use resonance_api::command::{ProviderKeyKind, ProviderKeyStatus, Request, Response};
use resonance_api::error::ErrorCode;
use resonance_core::providers::ProviderKey;
use resonance_db::Database;
use resonance_identify::credentials::{CredentialStore, MemoryCredentialStore};

const SECRET: &str = "test-key-0123456789";

fn api() -> (Api, Arc<MemoryCredentialStore>) {
    let store = Arc::new(MemoryCredentialStore::new());
    let api = Api::new_silent(Database::open_in_memory().unwrap())
        .with_credential_store(Arc::clone(&store) as Arc<dyn CredentialStore>);
    (api, store)
}

fn call(api: &Api, json: &str) -> String {
    let request: Request = serde_json::from_str(json).unwrap();
    let reply = api.handle(request);
    match reply {
        Ok(response) => serde_json::to_string(&response).unwrap(),
        Err(error) => serde_json::to_string(&error).unwrap(),
    }
}

fn status(api: &Api) -> Vec<ProviderKeyStatus> {
    match api.handle(Request::ProviderKeys).unwrap() {
        Response::ProviderKeys(keys) => keys,
        other => panic!("expected provider keys, got {other:?}"),
    }
}

#[test]
fn every_key_starts_not_set_and_paid_keys_are_marked() {
    let (api, _) = api();
    let keys = status(&api);
    assert_eq!(keys.len(), 3);
    assert!(keys.iter().all(|key| !key.configured));
    assert_eq!(
        keys.iter()
            .filter(|key| key.paid)
            .map(|key| key.provider)
            .collect::<Vec<_>>(),
        vec![ProviderKeyKind::AudD]
    );
}

#[test]
fn saving_and_clearing_a_key_reports_only_whether_it_is_set() {
    let (api, store) = api();

    let saved = call(
        &api,
        &format!(
            r#"{{"command":"set_provider_key","params":{{"provider":"acoustid","value":"  {SECRET}  "}}}}"#
        ),
    );
    assert!(!saved.contains(SECRET), "reply leaked the key: {saved}");
    assert!(saved.contains(r#""configured":true"#));
    assert_eq!(
        store
            .read(ProviderKey::AcoustId)
            .unwrap()
            .map(|secret| secret.expose().to_owned()),
        Some(SECRET.to_owned())
    );

    let listed = call(&api, r#"{"command":"provider_keys"}"#);
    assert!(!listed.contains(SECRET));
    let keys = status(&api);
    assert!(
        keys.iter()
            .any(|key| key.provider == ProviderKeyKind::AcoustId && key.configured)
    );
    assert!(
        keys.iter()
            .any(|key| key.provider == ProviderKeyKind::Discogs && !key.configured)
    );

    let cleared = call(
        &api,
        r#"{"command":"clear_provider_key","params":{"provider":"acoustid"}}"#,
    );
    assert!(cleared.contains(r#""configured":false"#));
    assert!(store.read(ProviderKey::AcoustId).unwrap().is_none());
}

#[test]
fn a_rejected_key_is_not_stored_or_repeated() {
    let (api, store) = api();
    let request: Request = serde_json::from_str(&format!(
        r#"{{"command":"set_provider_key","params":{{"provider":"discogs","value":"{SECRET}\n{SECRET}"}}}}"#
    ))
    .unwrap();
    assert!(!format!("{request:?}").contains(SECRET));

    let error = api.handle(request).unwrap_err();
    assert_eq!(error.code, ErrorCode::BadRequest);
    assert!(!error.message.contains(SECRET));
    assert!(store.read(ProviderKey::Discogs).unwrap().is_none());

    let blank = call(
        &api,
        r#"{"command":"set_provider_key","params":{"provider":"discogs","value":"   "}}"#,
    );
    assert!(blank.contains("bad_request"));
}
