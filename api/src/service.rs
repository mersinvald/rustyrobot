use failure::Error;
use gh::StatusCode;
use gh::query::Query;
use gh::mutation::Mutation;
use json::Value;
use std::fmt::{Display, Debug};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::sync::{Arc, Mutex, mpsc};
use std::borrow::Cow;
use std::thread;

use error_chain_failure_interop::ResultExt;
use search::*;
use search::query::*;

use github::GitHub;
use github::Request;

pub struct GithubService {
    token: String,
    clients: Vec<Client>
}

impl GithubService {
    pub fn new(token: &str) -> Self {
        GithubService {
            token: token.to_string(),
            clients: vec![]
        }
    }

    pub fn handle<T>(&mut self, custom_id: Option<T>) -> Handle
        where T: Into<Cow<'static, str>>
    {
        let (req_tx, req_rx) = mpsc::channel();
        let (resp_tx, resp_rx) = mpsc::channel();

        let id = custom_id.map(Into::into);
        let id = id.unwrap_or_else(|| {
            Cow::from(format!("Unnamed {}",  self.clients.len() + 1))
        });

        info!("registering new handle for {}", id);

        let client = Client {
            id,
            req_rx,
            resp_tx,
        };

        let handle = Handle {
            id,
            req_tx,
            resp_rx,
        };

        self.clients.push(client);
        handle
    }

    pub fn start(self) {
        debug!("starting github client thread");
        thread::spawn(|| self.thread_main());
    }

    fn thread_main(self) {
        info!("started github client thread");
        let clients = self.clients;
        let gh = GitHub::new(self.token);

        // Remove list for clients who hanged up
        let mut hanged_up = vec![];

        for client in clients {
            let request = match client.req_rx.recv() {

            }
        }

    }
}

pub type Response = Result<Value, Error>;
type RequestTx = mpsc::Sender<Request>;
type RequestRx = mpsc::Receiver<Request>;
type ResponseTx = mpsc::Sender<Response>;
type ResponseRx = mpsc::Receiver<Response>;

pub struct Client {
    id: Cow<'static, str>,
    req_rx: RequestRx,
    resp_tx: ResponseTx,
}

pub struct Handle {
    id: Cow<'static, str>,
    req_tx: RequestTx,
    resp_rx: ResponseRx,
}
