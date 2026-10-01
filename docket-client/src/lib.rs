//! What a client of the docket server needs: where it is, the key, and its routes as typed calls.

pub mod api;
pub mod config;

pub use api::{Api, Changes, Error};
pub use config::Config;
