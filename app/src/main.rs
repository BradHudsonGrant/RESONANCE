#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::fmt;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use clap::Parser;
use resonance_api::Api;
use resonance_api::command::{Request, Response};
use resonance_api::data_folder::default_data_folder;
use resonance_api::error::ApiError;
use resonance_db::{Database, FILE_NAME};
use tauri::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use tauri::http::{Request as ProtocolRequest, Response as ProtocolResponse, StatusCode};
use tauri::utils::config::AppDirectoriesOverride;
use tauri::webview::NewWindowResponse;
use tauri::{Emitter, Manager, State, Url, WebviewUrl, WebviewWindowBuilder};

#[derive(Parser)]
#[command(name = "resonance-app", version, about = "Resonance for the desktop")]
struct Options {
    /// Keep the library and the webview's data in this folder instead of the platform's app folder
    #[arg(long, value_name = "FOLDER")]
    data_folder: Option<PathBuf>,
}

#[derive(Debug)]
enum AppError {
    DataFolder(ApiError),
    Window(tauri::Error),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DataFolder(source) => source.fmt(f),
            Self::Window(source) => ApiError::internal(source).fmt(f),
        }
    }
}

impl std::error::Error for AppError {}

#[tauri::command]
async fn dispatch(
    request: Request,
    api: State<'_, Result<Arc<Api>, ApiError>>,
) -> Result<Response, ApiError> {
    let api = match api.inner() {
        Ok(api) => Arc::clone(api),
        Err(error) => return Err(error.clone()),
    };
    tauri::async_runtime::spawn_blocking(move || api.handle(request))
        .await
        .map_err(ApiError::internal)?
}

#[tauri::command]
async fn pick_audio_file() -> Result<Option<String>, ApiError> {
    tauri::async_runtime::spawn_blocking(|| {
        Ok(rfd::FileDialog::new()
            .set_title("Locate audio file")
            .add_filter(
                "Audio files",
                &["wav", "aiff", "aif", "flac", "mp3", "m4a", "mp4"],
            )
            .pick_file()
            .map(|path| path.to_string_lossy().into_owned()))
    })
    .await
    .map_err(ApiError::internal)?
}

#[tauri::command]
async fn pick_folder() -> Result<Option<String>, ApiError> {
    tauri::async_runtime::spawn_blocking(|| {
        Ok(rfd::FileDialog::new()
            .set_title("Select folder to search")
            .pick_folder()
            .map(|path| path.to_string_lossy().into_owned()))
    })
    .await
    .map_err(ApiError::internal)?
}

#[tauri::command]
async fn pick_playlist_file() -> Result<Option<String>, ApiError> {
    tauri::async_runtime::spawn_blocking(|| {
        Ok(rfd::FileDialog::new()
            .set_title("Import playlist")
            .add_filter("M3U playlists", &["m3u", "m3u8"])
            .pick_file()
            .map(|path| path.to_string_lossy().into_owned()))
    })
    .await
    .map_err(ApiError::internal)?
}

#[tauri::command]
async fn pick_playlist_save_path(
    folder: Option<String>,
    name: String,
) -> Result<Option<String>, ApiError> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut dialog = rfd::FileDialog::new()
            .set_title("Export to M3U8")
            .add_filter("M3U8 playlist", &["m3u8"])
            .set_file_name(name);
        if let Some(folder) = folder {
            dialog = dialog.set_directory(folder);
        }
        Ok(dialog
            .save_file()
            .map(|path| path.to_string_lossy().into_owned()))
    })
    .await
    .map_err(ApiError::internal)?
}

fn validate_external_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.len() > 2048
        || trimmed.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(
            "External URL must be a single line without control characters or spaces.".to_owned(),
        );
    }
    if trimmed.contains(['"', ',', '\\']) {
        return Err("External URL contains unsupported punctuation.".to_owned());
    }
    let parsed = Url::parse(trimmed).map_err(|_| "External URL is not valid.".to_owned())?;
    if parsed.scheme() != "https" {
        return Err("Only https:// URLs may be opened externally.".to_owned());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("External URL must not include userinfo.".to_owned());
    }
    let Some(host) = parsed.domain() else {
        return Err("External URL must include a valid domain host.".to_owned());
    };
    if !host.contains('.')
        || host.split('.').any(|label| label.is_empty())
        || host.eq_ignore_ascii_case("localhost")
        || host.to_ascii_lowercase().ends_with(".localhost")
    {
        return Err("External URL host is not valid.".to_owned());
    }
    let clean = parsed.as_str();
    if clean.contains(['"', ',', '\\']) {
        return Err("External URL contains unsupported punctuation.".to_owned());
    }
    Ok(clean.to_owned())
}

#[tauri::command]
async fn open_external_url(url: String) -> Result<(), ApiError> {
    let clean = validate_external_url(&url).map_err(ApiError::invalid_input)?;
    tauri::async_runtime::spawn_blocking(move || {
        #[cfg(target_os = "windows")]
        let status = std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", &clean])
            .status();
        #[cfg(target_os = "macos")]
        let status = std::process::Command::new("open").arg(&clean).status();
        #[cfg(all(unix, not(target_os = "macos")))]
        let status = std::process::Command::new("xdg-open").arg(&clean).status();
        let exit = status.map_err(ApiError::internal)?;
        if !exit.success() {
            return Err(ApiError::internal(
                "Could not open link in default browser.",
            ));
        }
        Ok(())
    })
    .await
    .map_err(ApiError::internal)?
}

/// A stored picture for `artwork://<hash>`, the desktop's way to the same files the local
/// socket serves at `/artwork/<hash>`.
fn artwork_response(
    api: Option<&Api>,
    request: &ProtocolRequest<Vec<u8>>,
) -> ProtocolResponse<Vec<u8>> {
    let hash = request.uri().path().trim_start_matches('/');
    let found = api
        .and_then(|api| api.artwork_file(hash))
        .and_then(|file| Some((file.mime, std::fs::read(&file.path).ok()?)));
    match found {
        Some((mime, bytes)) => ProtocolResponse::builder()
            .header(CONTENT_TYPE, mime)
            .header(CACHE_CONTROL, "private, max-age=31536000, immutable")
            .body(bytes)
            .unwrap_or_default(),
        None => ProtocolResponse::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Vec::new())
            .unwrap_or_default(),
    }
}

fn local_page(url: &Url) -> bool {
    url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && matches!(
            (url.scheme(), url.host_str()),
            ("tauri", Some("localhost")) | ("https", Some("tauri.localhost"))
        )
}

fn run(options: Options) -> Result<(), AppError> {
    let folder = options
        .data_folder
        .or_else(default_data_folder)
        .ok_or_else(|| AppError::DataFolder(ApiError::data_folder_unavailable()))?;
    let folder = std::path::absolute(folder)
        .map_err(|source| AppError::DataFolder(ApiError::internal(source)))?;
    let api = Database::open(&folder.join(FILE_NAME))
        .map(|library| Arc::new(Api::new(library).with_data_folder(folder.clone())))
        .map_err(ApiError::library_unavailable);

    let watch_revisions = api.as_ref().ok().map(|api| api.watch_revisions());
    let activity_revisions = api.as_ref().ok().map(|api| api.activity_revisions());

    let mut context = tauri::generate_context!();
    context.config_mut().app.app_directories_override = Some(AppDirectoriesOverride::Root(folder));

    tauri::Builder::default()
        .manage(api)
        .register_asynchronous_uri_scheme_protocol("artwork", |context, request, responder| {
            let api = context
                .app_handle()
                .state::<Result<Arc<Api>, ApiError>>()
                .inner()
                .as_ref()
                .ok()
                .map(Arc::clone);
            tauri::async_runtime::spawn_blocking(move || {
                responder.respond(artwork_response(api.as_deref(), &request));
            });
        })
        .invoke_handler(tauri::generate_handler![
            dispatch,
            pick_audio_file,
            pick_folder,
            pick_playlist_file,
            pick_playlist_save_path,
            open_external_url
        ])
        .setup(move |app| {
            let window =
                WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                    .title("Resonance")
                    .inner_size(1440.0, 900.0)
                    .min_inner_size(760.0, 480.0)
                    .use_https_scheme(true)
                    .on_navigation(local_page)
                    .on_new_window(|_, _| NewWindowResponse::Deny)
                    .build()?;
            if let Some(mut revisions) = watch_revisions {
                let library_window = window.clone();
                tauri::async_runtime::spawn(async move {
                    while revisions.changed().await.is_ok() {
                        let revision = *revisions.borrow_and_update();
                        if library_window
                            .emit("resonance://library-changed", revision)
                            .is_err()
                        {
                            break;
                        }
                    }
                });
            }
            if let Some(mut revisions) = activity_revisions {
                tauri::async_runtime::spawn(async move {
                    while revisions.changed().await.is_ok() {
                        let revision = *revisions.borrow_and_update();
                        if window
                            .emit("resonance://activity-changed", revision)
                            .is_err()
                        {
                            break;
                        }
                    }
                });
            }
            Ok(())
        })
        .run(context)
        .map_err(AppError::Window)
}

fn main() -> ExitCode {
    match run(Options::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            rfd::MessageDialog::new()
                .set_title("Resonance could not start")
                .set_description(error.to_string())
                .set_level(rfd::MessageLevel::Error)
                .show();
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_only_navigates_to_bundled_pages() {
        for address in [
            "tauri://localhost/index.html",
            "https://tauri.localhost/index.html#library",
        ] {
            assert!(local_page(&address.parse().unwrap()), "{address}");
        }

        for address in [
            "https://example.com",
            "https://tauri.localhost.example.com",
            "https://tauri.localhost:5173",
            "https://user@tauri.localhost",
            "http://tauri.localhost",
            "http://localhost:5173",
            "tauri://example.com/index.html",
            "file:///index.html",
            "data:text/html,hello",
        ] {
            assert!(!local_page(&address.parse().unwrap()), "{address}");
        }
    }

    #[test]
    fn a_data_folder_can_be_named_without_changing_the_platform_default() {
        let options =
            Options::try_parse_from(["resonance-app", "--data-folder", "target/dev/desktop"])
                .unwrap();

        assert_eq!(options.data_folder, Some("target/dev/desktop".into()));
        assert!(
            Options::try_parse_from(["resonance-app"])
                .unwrap()
                .data_folder
                .is_none()
        );
    }

    #[test]
    fn external_url_validation_allows_https_and_blocks_unsafe_schemes_or_chars() {
        assert_eq!(
            validate_external_url(" https://www.discogs.com/release/123 "),
            Ok("https://www.discogs.com/release/123".to_owned())
        );
        for bad in [
            "",
            "http://example.com",
            "file:///C:/Windows/System32/calc.exe",
            "javascript:alert(1)",
            "https://example.com/a b",
            "https://example.com/\nfoo",
            "https://user@example.com",
            "https://localhost/test",
            "https://sub.localhost/test",
            "https://127.0.0.1/release/1",
            "https://example.com:abc/",
            "https://a..b.com/test",
            "https://example.com/a,b",
            "https://example.com/a\"b",
            "https://example.com/a\\b",
        ] {
            assert!(validate_external_url(bad).is_err(), "{bad}");
        }
    }
}
