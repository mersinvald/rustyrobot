pub mod v3;
pub mod v4;
pub mod utils;

use serde::de::DeserializeOwned;
use failure::Error;

pub trait GithubClient {
    type Request;
    fn request<T>(&self, request: &Self::Request) -> Result<T, Error>
        where T: DeserializeOwned;
}

#[derive(Fail, Debug)]
pub enum RequestError {
    #[fail(display = "server returned status {}, expected 200", status)]
    ResponseStatusNotOk { status: u16 },
    #[fail(display = "server returned empty json response")]
    EmptyResponse,
    #[fail(display = "invalid json schema:\n\texpected {:?}\n\tgot {:?}", expected, got)]
    InvalidJson { expected: String, got: String },
    #[fail(display = "exceeded rate limit: retry in {} seconds", retry_in)]
    ExceededRateLimit { retry_in: u64 }
}
