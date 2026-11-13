pub mod command;
pub mod error;
pub mod serve;

use command::{AppInfo, Request, Response};
use error::ApiError;

/// Everything the commands act on. The Tauri shell and the local socket each hold one.
#[derive(Debug, Default)]
pub struct Api;

impl Api {
    pub fn new() -> Self {
        Self
    }

    pub fn handle(&self, request: Request) -> Result<Response, ApiError> {
        match request {
            Request::AppInfo => Ok(Response::AppInfo(AppInfo {
                version: env!("CARGO_PKG_VERSION").to_owned(),
            })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_info_reports_the_version_of_the_core() {
        let response = Api::new().handle(Request::AppInfo).unwrap();

        assert_eq!(
            response,
            Response::AppInfo(AppInfo {
                version: env!("CARGO_PKG_VERSION").to_owned()
            })
        );
    }
}
