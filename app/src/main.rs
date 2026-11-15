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
use tauri::utils::config::AppDirectoriesOverride;
use tauri::webview::NewWindowResponse;
use tauri::{Emitter, State, Url, WebviewUrl, WebviewWindowBuilder};

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

    let mut context = tauri::generate_context!();
    context.config_mut().app.app_directories_override = Some(AppDirectoriesOverride::Root(folder));

    tauri::Builder::default()
        .manage(api)
        .invoke_handler(tauri::generate_handler![dispatch])
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
                tauri::async_runtime::spawn(async move {
                    while revisions.changed().await.is_ok() {
                        let revision = *revisions.borrow_and_update();
                        if window
                            .emit("resonance://library-changed", revision)
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
}
