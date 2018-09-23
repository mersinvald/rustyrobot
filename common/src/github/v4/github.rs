use failure::Error;
use gh4::StatusCode;
use gh4::query::Query;
use gh4::mutation::Mutation;
use json::Value;
use std::fmt::{Display, Debug};
use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use std::sync::{Arc, Mutex, mpsc};
use std::sync::mpsc::TryRecvError;
use std::borrow::Cow;
use std::thread;
use std::time::Duration;

use error_chain_failure_interop::ResultExt;
use search::*;
use search::query::*;

use github::v4::client::Client;
use github::v4::client::Request;
use github::v4::client::RequestType;
use github::RequestError;
use github::GithubClient;

use db::KV;

pub struct Github {
    client: Client,
}

impl Github {
    pub fn new(db: KV, token: &str) -> Result<Self, Error> {
        Ok(Github {
            client: Client::new(db, token)?
        })
    }

    pub fn query<T, U, R>(&self, description: T, query: U) -> Result<R, Error>
        where T: Into<Cow<'static, str>>,
              U: Into<Cow<'static, str>>,
              R: DeserializeOwned
    {
        let request = Request {
            description: description.into(),
            body: RequestType::Query(query.into())
        };

        self.request(&request)
    }

    pub fn mutate<T, R>(&self, description: T, query: T) -> Result<Value, Error>
        where T: Into<Cow<'static, str>>,
              R: DeserializeOwned,
    {
        let request = Request {
            description: description.into(),
            body: RequestType::Mutation(query.into())
        };

        self.request(&request)
    }
}

impl GithubClient for Github {
    type Request = Request;
    fn request<T>(&self, request: &Self::Request) -> Result<T, Error>
        where T: DeserializeOwned
    {
        // Request timeout retry loop
        let request_result = loop {
            match self.client.request(request) {
                Ok(resp) => break Ok(resp),
                Err(err) => match err.downcast::<RequestError>() {
                    // If timeout error, sleep and continue the loop
                    Ok(RequestError::ExceededRateLimit { ref retry_in }) => {
                        warn!("exceeded rate limit: retrying in {} seconds", retry_in);
                        thread::sleep(Duration::from_secs(*retry_in));
                    },
                    // If other downcast variant or downcast failed -- break with error
                    Ok(err) => break Err(Error::from(err)),
                    Err(err) => break Err(err),
                }
            }
        };

        match request_result {
            Ok(_) => info!("request {:?} finished successfully", request.description),
            Err(ref err) => error!("request {:?} finished with error: {}", request.description, err),
        }

        request_result
    }
}


