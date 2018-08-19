use failure::Error;
use gh4::StatusCode;
use gh4::query::Query;
use gh4::mutation::Mutation;
use json::Value;
use std::fmt::{Display, Debug};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::sync::{Arc, Mutex, mpsc};
use std::sync::mpsc::TryRecvError;
use std::borrow::Cow;
use std::thread;
use std::time::Duration;

use error_chain_failure_interop::ResultExt;
use search::*;
use search::query::*;

use github::v4::GitHub;
use github::v4::Request;
use github::v4::RequestError;
use github::v4::RequestType;
use github::RequestCost;

use db::KV;

pub struct GithubService {
    db: KV,
    token: String,
    clients: Vec<Client>
}

impl GithubService {
    pub fn new(db: KV, token: &str) -> Self {
        GithubService {
            db,
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
            id: id.clone(),
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

    pub fn start(self) -> Result<(), Error> {
        debug!("starting github client thread");

        // channel to pass back client initialization error
        let (status_tx, status_rx) = mpsc::channel();
        thread::spawn(|| self.thread_main(status_tx));

        // Block for some time in case client initialization failed
        if let Ok(Err(err)) = status_rx.recv() {
            Err(err)
        } else {
            Ok(())
        }
    }

    fn thread_main(self, status_tx: ErrorTx) {
        info!("started github client thread");
        let mut clients = self.clients;
        let mut gh = match GitHub::new(self.db, self.token) {
            // Pass initialisation status to the parent thread through channel
            Ok(gh) => {
                status_tx.send(Ok(())).unwrap();
                gh
            },
            Err(err) => {
                status_tx.send(Err(err)).unwrap();
                return
            }
        };

        // Remove list for clients who hanged up
        let mut hanged_up = vec![];

        // Main loop
        loop {
            // Pause flag
            let mut should_pause = true;

            for (n, client) in clients.iter().enumerate() {
                trace!("polling client {:?} message queue", client.id);
                let req = match client.req_rx.try_recv() {
                    Ok(req) => req,
                    Err(TryRecvError::Empty) => continue,
                    Err(TryRecvError::Disconnected) => {
                        hanged_up.push(n);
                        continue
                    }
                };
                debug!("accepted request from client {:?}", client.id);

                // Disable pauses for one iteration after each live request
                should_pause = false;

                // Request timeout retry loop
                let request_result = loop {
                    match gh.request::<Value>(&req) {
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
                    Ok(_) => info!("request handling for client {:?} finished successfully", client.id),
                    Err(ref err) => error!("request handling for client {:?} finished with error: {}", client.id, err),
                }


                // If send failed client hang up
                if client.resp_tx.send(request_result).is_err() {
                    hanged_up.push(n)
                }
            }

            // Remove hanged-up clients
            for idx in hanged_up.drain(..) {
                warn!("client {:?} hang up", clients[idx].id);
                clients.swap_remove(idx);
            }

            // Clear the hangs-up list
            hanged_up.clear();

            if should_pause {
                thread::sleep(Duration::from_millis(100));
            }
        }
        info!("shutting down github client thread");
    }
}

pub type ResponseResult = Result<Value, Error>;
type RequestTx = mpsc::Sender<Request>;
type RequestRx = mpsc::Receiver<Request>;
type ResponseTx = mpsc::Sender<ResponseResult>;
type ResponseRx = mpsc::Receiver<ResponseResult>;
type ErrorTx = mpsc::Sender<Result<(), Error>>;
type ErrorRx = mpsc::Receiver<Result<(), Error>>;

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

static SERVICE_HANG_UP_PANIC_MSG: &str = "Service channel is closed. Make sure you ::start GitHubService before using the handle.";

impl Handle {
    pub fn query<T, U>(&self, description: T, cost: RequestCost, query: U) -> Result<Value, Error>
        where T: Into<Cow<'static, str>>,
              U: Into<Cow<'static, str>>,
    {
        let request = Request {
            cost,
            description: description.into(),
            body: RequestType::Query(query.into())
        };

        self.request(request)
    }

    pub fn mutate<T>(&self, description: T, cost: RequestCost, query: T) -> Result<Value, Error>
        where T: Into<Cow<'static, str>>
    {
        let request = Request {
            cost,
            description: description.into(),
            body: RequestType::Mutation(query.into())
        };

        self.request(request)
    }

    pub fn request(&self, request: Request) -> Result<Value, Error> {
        self.req_tx.send(request).expect(SERVICE_HANG_UP_PANIC_MSG);
        self.resp_rx.recv().expect(SERVICE_HANG_UP_PANIC_MSG)
    }
}
