//! Working out what a file is: the six-step cascade from the file's own tags to the release
//! pages a user pastes, the providers it asks, and the keys they need.

pub mod credentials;
pub mod http;
pub mod library;
pub mod pipeline;
pub mod provider;
pub mod providers;
pub mod rate;
