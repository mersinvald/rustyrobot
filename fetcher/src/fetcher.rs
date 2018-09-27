use strategy::{Strategy, DateWindow};

use std::sync::Arc;

use failure::Error;
use std::io::Write;
use std::fs::File;

use std::time::{Instant, Duration};


use rustyrobot::{
    search::{query::{Query, IncompleteQuery, Lang}},
    types::Repository,
    shutdown::GracefulShutdownHandle,
    kafka::util::{
        state::StateHandler,
        producer::ThreadedProducerHandle,
    }
};

use std::thread;
use chrono::Utc;
use std::mem::discriminant;
use std::borrow::Cow;

use chrono::NaiveDate;

pub struct FetcherState<'a> {
    pub shutdown: GracefulShutdownHandle,
    pub state: &'a mut StateHandler,
    pub producer: ThreadedProducerHandle,
}

pub struct Fetcher<'a, S: Strategy> {
    state: FetcherState<'a>,
    strategy: S,
}

impl<'a> Fetcher<'a, DateWindow> {
    pub fn new_with_default_strategy(state: &'a mut StateHandler, producer: ThreadedProducerHandle, shutdown: GracefulShutdownHandle) -> Self {
        Fetcher::new(
            state,
            producer,
            shutdown,
            DateWindow {
                days_per_request: 1,
                ..Default::default()
            }
        )
    }
}

impl<'a, S: Strategy> Fetcher<'a, S> {
    pub fn new(state: &'a mut StateHandler, producer: ThreadedProducerHandle, shutdown: GracefulShutdownHandle, strategy: S) -> Self {
        Fetcher {
            state: FetcherState {
                shutdown,
                state,
                producer,
            },
            strategy,
        }
    }

    pub fn fetch(&mut self, base_query: IncompleteQuery) -> Result<(), Error> {
        Ok(self.strategy.execute(&mut self.state, base_query)?)
    }
}


