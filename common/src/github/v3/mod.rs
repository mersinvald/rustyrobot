pub use gh3::client::Github;
use gh3::StatusCode;
use gh3::headers::{
    rate_limit,
    rate_limit_remaining,
    rate_limit_reset
};

use std::borrow::Cow;
use std::sync::{Arc, RwLock};

use serde::de::DeserializeOwned;
use failure::Error;
use json::{self, Value};
use gh3::client::Executor;
use github::RequestError;
use github::utils;

use error_chain_failure_interop::ResultExt;

use chrono::Utc;
use chrono::NaiveDate;
use std::thread;
use std::time::Duration;
use std::fmt::Debug;

pub trait ExecutorExt<T>: Executor + Sized {
    fn send(self, good_statuses: &[StatusCode]) -> Result<T, Error>;
}

impl<T, E: Executor + Sized> ExecutorExt<T> for E
    where T: DeserializeOwned 
{
    fn send(self, good_statuses: &[StatusCode]) -> Result<T, Error>
        where T: DeserializeOwned
    {
        let now = Utc::now().timestamp();
        let limits = *RATE_LIMIT.read().unwrap();
        if limits.remaining < LIMIT_THRESHOLD && now < limits.reset_at {
            let timeout = limits.reset_at - Utc::now().timestamp();
            warn!("request limit exceeded: retrying in {} seconds", timeout);
            thread::sleep(Duration::from_secs(timeout as u64));
        }

        // Perform request
        let (headers, status, data) = self.execute::<Value>().sync()?;

        trace!("status: {}", status);
        if data.is_none() {
            trace!("response: empty");
        }

        // Get rate limits info
        let rate_limit = || -> Option<RateLimit> {
            let limits = RateLimit {
                limit: rate_limit(&headers)?,
                remaining: rate_limit_remaining(&headers)?,
                reset_at: rate_limit_reset(&headers)? as i64,
            };
            debug!("API v3 limits: {:?}", limits);
            Some(limits)
        }();

        if let Some(rate_limit) = rate_limit {
            *RATE_LIMIT.write().unwrap() = rate_limit;
        }

        // Handle the response
        let json = data.ok_or(RequestError::EmptyResponse)?;
        trace!("response: {}", json);

        if utils::is_rate_limit_error_v3(status, &json) {
            // Raise unfilled rate limit error
            raise!(RequestError::ExceededRateLimit { retry_in: 0 })
        }

        if !good_statuses.contains(&status) {
            raise!(RequestError::ResponseStatusNotOk { status: status.as_u16() })
        }

        Ok(json::from_value(json)?)
    }
}

pub struct EmptyResponse;

impl<E: Executor + Sized> ExecutorExt<EmptyResponse> for E {
    fn send(self, good_statuses: &[StatusCode]) -> Result<EmptyResponse, Error> {
        let now = Utc::now().timestamp();
        let limits = *RATE_LIMIT.read().unwrap();
        if limits.remaining < LIMIT_THRESHOLD && now < limits.reset_at {
            let timeout = limits.reset_at - Utc::now().timestamp();
            warn!("request limit exceeded: retrying in {} seconds", timeout);
            thread::sleep(Duration::from_secs(timeout as u64));
        }

        // Perform request
        let (headers, status, data) = self.execute::<Value>().sync()?;

        trace!("status: {}", status);
        if data.is_none() {
            trace!("response: empty");
        }

        // Get rate limits info
        let rate_limit = || -> Option<RateLimit> {
            let limits = RateLimit {
                limit: rate_limit(&headers)?,
                remaining: rate_limit_remaining(&headers)?,
                reset_at: rate_limit_reset(&headers)? as i64,
            };
            debug!("API v3 limits: {:?}", limits);
            Some(limits)
        }();

        if let Some(rate_limit) = rate_limit {
            *RATE_LIMIT.write().unwrap() = rate_limit;
        }

        if !good_statuses.contains(&status) {
            raise!(RequestError::ResponseStatusNotOk { status: status.as_u16() })
        }

        Ok(EmptyResponse)
    }
}

#[derive(Copy, Clone, Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RateLimit {
    limit: u32,
    remaining: u32,
    reset_at: i64,
}

// Remaining requests threshold. Under this value requests will be stopped until limits reset
const LIMIT_THRESHOLD: u32 = 5;
lazy_static! {
    static ref RATE_LIMIT: Arc<RwLock<RateLimit>> = Arc::new(RwLock::new(RateLimit::default()));
}
