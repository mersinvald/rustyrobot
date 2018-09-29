use failure::Error;
use gh4::StatusCode;
use gh4::client::Github as Driver;
use gh4::query::Query;
use json::Value;
use std::fmt::Display;
use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use std::borrow::Cow;
use error_chain_failure_interop::ResultExt;
use json;
use github::utils;
use github::GithubClient;
use github::RequestError;
use std::cell::RefCell;

pub struct Client {
    driver: RefCell<Driver>,
    login: String,
    limit: RateLimit,
}

pub enum RequestType {
    Query(Cow<'static, str>),
    Mutation(Cow<'static, str>),
}

pub struct Request {
    pub description: Cow<'static, str>,
    pub body: RequestType,
}

// TODO: statistics

impl GithubClient for Client {
    type Request = Request;
    fn request<T>(&self, request: &Request) -> Result<T, Error>
        where T: DeserializeOwned
    {
        let description = &request.description;
        let result = match &request.body {
            RequestType::Query(query) => {
                Self::run_query::<_, &str>(&mut self.driver.borrow_mut(), description, query, None)
            },
            RequestType::Mutation(_query) => {
                unimplemented!()
            }
        };

        match result {
            Ok(data) => {
                trace!("request succeeded");
                Ok(data)
            },
            Err(err) => {
                error!("{} request failed: {}", description, err);
                match err.downcast::<RequestError>() {
                    Ok(err) => Err(self.fill_rate_limit_error(err)?),
                    Err(err) => Err(err),
                }
            }
        }
    }
}

impl Client {
    pub fn new<T>(token: T) -> Result<Self, Error>
        where T: AsRef<str> + Display
    {
        let mut driver = Driver::new(token)
            .sync()?;

        let login = Self::run_get_login(&mut driver)?;

        let limit = Self::run_get_api_limit(&mut driver)?;

        let driver = RefCell::new(driver);

        let gh = Client {
            driver,
            login,
            limit
        };

        Ok(gh)
    }

    fn fill_rate_limit_error(&self, error: RequestError) -> Result<Error, Error> {
        match error {
            RequestError::ExceededRateLimit { .. } => {
                let now = Utc::now();
                let retry_in = self.limit.reset_at.timestamp() - now.timestamp();
                assert!(retry_in >= 0);
                Err(RequestError::ExceededRateLimit { retry_in: retry_in as u64 }.into())
            },
            err => Err(err.into()),
        }
    }

    fn run_get_login(driver: &mut Driver) -> Result<String, Error> {
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

    fn run_get_api_limit(driver: &mut Driver) -> Result<RateLimit, Error> {
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

    fn run_query<T, S>(driver: &mut Driver, description: &str, query: &str, json_selectors: Option<&[&S]>) -> Result<T, Error>
        where T: DeserializeOwned,
              S: json::value::Index,
    {
        let (_, status, json) = driver.query::<Value>(
            &Query::new_raw(query)
        ).sync()?;

        debug!("{} status: {}", description, status);
        let mut json = json.ok_or(RequestError::EmptyResponse)?;
        trace!("{} response: {}", description, json);

        if utils::is_rate_limit_error_v4(status, &json) {
            // Raise unfilled rate limit error
            raise!(RequestError::ExceededRateLimit { retry_in: 0 })
        }

        match status {
            StatusCode::Ok => (),
            status => raise!(RequestError::ResponseStatusNotOk { status: status.as_u16() })
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
