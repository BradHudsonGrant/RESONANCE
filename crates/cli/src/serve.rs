use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use resonance_api::Api;
use resonance_api::serve::Server;
use serde_json::json;

use crate::library_file::{self, OpenError};

#[derive(Debug)]
pub enum ServeError {
    Library(OpenError),
    Start(io::Error),
    ConnectionFile { path: PathBuf, source: io::Error },
    Stopped(io::Error),
}

impl fmt::Display for ServeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Library(source) => source.fmt(f),
            Self::Start(source) => write!(f, "The core could not start listening: {source}"),
            Self::ConnectionFile { path, source } => {
                write!(
                    f,
                    "The connection file {} could not be written: {source}",
                    path.display()
                )
            }
            Self::Stopped(source) => write!(f, "The core stopped serving: {source}"),
        }
    }
}

impl std::error::Error for ServeError {}

pub fn run(connection_file: Option<&Path>, data_folder: Option<&Path>) -> Result<(), ServeError> {
    let data_folder = library_file::data_folder(data_folder).map_err(ServeError::Library)?;
    let api = Api::new(library_file::open(&data_folder).map_err(ServeError::Library)?);
    let runtime = tokio::runtime::Runtime::new().map_err(ServeError::Start)?;
    let served =
        runtime.block_on(async {
            let server = Server::bind().await.map_err(ServeError::Start)?;
            let address = server.address().map_err(ServeError::Start)?;
            println!("Resonance core listening on ws://{address}");
            match connection_file {
                Some(path) => write_connection_file(path, address.port(), server.token()).map_err(
                    |source| ServeError::ConnectionFile {
                        path: path.to_owned(),
                        source,
                    },
                )?,
                None => println!("token {}", server.token()),
            }
            tokio::select! {
                stopped = server.run(Arc::new(api)) => stopped.map_err(ServeError::Stopped),
                _ = tokio::signal::ctrl_c() => Ok(()),
            }
        });
    if let Some(path) = connection_file {
        // A stale file would point the dev server at a core that is gone.
        let _ = fs::remove_file(path);
    }
    served
}

/// Written whole under another name and then renamed, so a reader never sees half a file.
fn write_connection_file(path: &Path, port: u16, token: &str) -> io::Result<()> {
    if let Some(folder) = path
        .parent()
        .filter(|folder| !folder.as_os_str().is_empty())
    {
        fs::create_dir_all(folder)?;
    }
    let mut partial = path.as_os_str().to_owned();
    partial.push(".partial");
    fs::write(
        &partial,
        json!({ "port": port, "token": token }).to_string(),
    )?;
    fs::rename(&partial, path)
}
