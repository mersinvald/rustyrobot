#[macro_use]
extern crate failure;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate log;
extern crate serde;
extern crate serde_json as json;
extern crate chrono;
extern crate dotenv;
extern crate github_gql as gh;
extern crate shell_escape;

#[macro_export]
macro_rules! json_get_chain {
    ($json:expr, $first:tt $(,$index:tt)*) => {
        $json.get($first)
        $(
            .and_then(|json| json.get($index))
        )*
    };
}

#[macro_use]
mod macros;
mod error_chain_failure_interop;
pub mod search;
pub mod github;
pub mod service;

pub static RESOURCES_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/res");

use failure::Error;
use gh::StatusCode;
use gh::client::Github;
use gh::query::Query;
use gh::mutation::Mutation;
use json::Value;
use std::fmt::{Display, Debug};
use chrono::{DateTime, Utc};
use serde::Deserialize;

use error_chain_failure_interop::ResultExt;
use search::*;
use search::query::*;

pub type Response = Result<ResponseBody, Error>;
pub type ResponseBody = Value;

use std::env;
pub fn load_token() -> Result<String, Error> {
    // First search .env
    let token = dotenv::var("GITHUB_TOKEN")
        // Then environment variables
        .or_else(|e| {
            env::var("GITHUB_TOKEN")
        })?;

    Ok(token)
}
