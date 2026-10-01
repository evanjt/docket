//! The server, through the shared client, its failures worded as the command prints them.

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use docket_client::{Api as Client, Config, Error};

use crate::fail::{Fail, Result};

pub struct Api {
    client: Client,
    base: String,
}

impl Api {
    /// # Errors
    /// The HTTP client cannot be built.
    pub fn new(config: &Config) -> Result<Self> {
        Ok(Self {
            client: Client::new(config).map_err(|e| failed(&config.server, &e))?,
            base: config.server.clone(),
        })
    }

    /// A read route's answer as it came.
    ///
    /// # Errors
    /// The server cannot be reached, or refuses.
    pub fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
        self.client
            .get(path, query)
            .map_err(|e| failed(&self.base, &e))
    }

    /// A write verb, its answer read as `T`.
    ///
    /// # Errors
    /// The server cannot be reached, refuses, or answers in another shape.
    pub fn post<T: DeserializeOwned>(&self, verb: &str, body: &impl Serialize) -> Result<T> {
        self.client
            .post(verb, body)
            .map_err(|e| failed(&self.base, &e))
    }
}

/// A refusal in the server's words; the key or the network in docket's.
fn failed(base: &str, e: &Error) -> Fail {
    match e {
        Error::Refused(401, _) => Fail::refused(format!("docket: {base} refused the key")),
        Error::Refused(_, why) => Fail::refused(why.clone()),
        Error::Failed(why) => Fail::refused(format!("docket: cannot reach the server: {why}")),
    }
}
