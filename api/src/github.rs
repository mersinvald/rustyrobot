use failure::Error;
use gh::StatusCode;
use gh::client::Github;
use gh::query::Query;
use gh::mutation::Mutation;
use json::Value;
use std::fmt::{Display, Debug};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::time::Duration;
use std::borrow::Cow;
use error_chain_failure_interop::ResultExt;
use search::*;
use search::query::*;
use json;
use db;
use db::stats;

pub struct GitHub {
    driver: Github,
    db: db::KV,
    login: String,
    limit: RateLimit,
}

pub enum RequestType {
    Query(Cow<'static, str>),
    Mutation(Cow<'static, str>),
}

pub struct Request {
    pub cost: RequestCost,
    pub description: Cow<'static, str>,
    pub body: RequestType,
}

#[derive(Copy, Clone, Debug)]
pub enum RequestCost {
    One,
    Custom(u64),
}

impl From<RequestCost> for u64 {
    fn from(rq: RequestCost) -> u64 {
        match rq {
            RequestCost::One => 1,
            RequestCost::Custom(x) => x
        }
    }
}

impl GitHub {
    pub fn new<T>(db: db::KV, token: T) -> Result<Self, Error>
        where T: AsRef<str> + Display
    {
        let mut driver = Github::new(token)
            .sync()?;

        let login = Self::run_get_login(&mut driver)?;

        let limit = Self::run_get_api_limit(&mut driver)?;

        let gh = GitHub {
            db,
            driver,
            login,
            limit
        };

        Ok(gh)
    }

    pub fn request<T>(&mut self, request: &Request) -> Result<T, Error>
        where T: for<'de> Deserialize<'de>
    {
        self.try_rate_limit(u64::from(request.cost))?;

        // Log statistics
        stats::increment_stat_counter(&self.db, "requests")?;
        stats::increment_stat_counter(&self.db, &format!("{}_requests", request.description))?;

        let description = &request.description;
        let result = match &request.body {
            RequestType::Query(query) => {
                Self::run_query::<_, &str>(&mut self.driver, description, query, None)
            },
            RequestType::Mutation(query) => {
                unimplemented!()
            }
        };

        match result {
            Ok(data) => {
                stats::increment_stat_counter(&self.db, "requests_succeeded")?;
                Ok(data)
            },
            Err(err) => {
                error!("{} request failed: {}", description, err);
                stats::increment_stat_counter(&self.db, "requests_failed")?;
                Err(err)
            }
        }
    }

    fn try_rate_limit(&self, cost: u64) -> Result<(), Error> {
        // Limit reserve to allow application to reauth and refetch the limits after restar
        static LIMIT_RESERVE: u64 = 6;
        if self.limit.used + cost + LIMIT_RESERVE >= self.limit.limit {
            stats::increment_stat_counter(&self.db, "request_limit_overflows")?;
            let now = Utc::now();
            let reset_in = self.limit.reset_at.timestamp() - now.timestamp();
            assert!(reset_in >= 0);
            Err(RequestError::ExceededRateLimit {
                used: self.limit.used,
                limit: self.limit.limit,
                retry_in: reset_in as u64
            }.into())
        } else {
            Ok(())
        }
    }

    fn run_get_login(driver: &mut Github) -> Result<String, Error> {
        info!("logging in via OAuth");

        let login: String = Self::run_query(
            driver,
            "login",
            "query { viewer { login } }",
            Some(&[&"data", &"viewer", &"login"])
        )?;

        info!("logged in as {:?}", login);
        Ok(login)
    }

    fn run_get_api_limit(driver: &mut Github) -> Result<RateLimit, Error> {
        info!("requesting rate limit");

        let mut limit: RateLimit = Self::run_query(
            driver,
            "rate limit",
            "query { rateLimit { limit remaining resetAt } }",
            Some(&[&"data", &"rateLimit"])
        )?;

        limit.used = limit.limit - limit.remaining;

        info!("rate limit: {}/hr", limit.limit);
        info!("used: {}", limit.used);
        info!("reset at: {}", limit.reset_at);

        Ok(limit)
    }

    fn run_query<T, S>(driver: &mut Github, description: &str, query: &str, json_selectors: Option<&[&S]>) -> Result<T, Error>
        where T: for<'de> Deserialize<'de>,
              S: json::value::Index,
    {
        let (_, status, json) = driver.query::<Value>(
            &Query::new_raw(query)
        ).sync()?;

        debug!("{} status: {}", description, status);
        let mut json = json.ok_or(RequestError::EmptyResponse)?;
        trace!("{} response: {}", description, json);

        match status {
            StatusCode::Ok => (),
            status => raise!(RequestError::ResponseStatusNotOk { status })
        }

        if let Some(selectors) = json_selectors {
            for selector in selectors {
                json = json[selector].take();
            }
        }

        Ok(json::from_value(json)?)
    }

}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RateLimit {
    limit: u64,
    remaining: u64,
    reset_at: DateTime<Utc>,
    #[serde(skip)]
    used: u64,
}

#[derive(Fail, Debug)]
pub enum RequestError {
    #[fail(display = "server returned status {}, expected 200 OK", status)]
    ResponseStatusNotOk { status: StatusCode },
    #[fail(display = "server returned empty json response")]
    EmptyResponse,
    #[fail(display = "invalid json schema:\n\texpected {:?}\n\tgot {:?}", expected, got)]
    InvalidJson { expected: String, got: String },
    #[fail(display = "exceeded rate limit:\n\tlimit: {}\n\tused: {}\n\tretry in {:?} seconds", limit, used, retry_in)]
    ExceededRateLimit { used: u64, limit: u64, retry_in: u64 }
}
