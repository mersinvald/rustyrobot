use strategy::Strategy;

use std::sync::Arc;

use failure::Error;
use std::io::Write;
use std::fs::File;

use std::time::{Instant, Duration};

use rocksdb::DB;
use api::db::KV;
use api::github::v4::Github;

use api::search::{search, NodeType, query::{Query, IncompleteQuery, Lang}};
use types::Repository;
use std::thread;
use chrono::Utc;
use api::db::stats;
use std::mem::discriminant;
use std::borrow::Cow;

use shutdown::GracefulShutdownHandle;

use chrono::NaiveDate;

type Hook<N> = Box<Fn(&N)>;

struct FetcherState<'s, N> {
    db: &'s DB,
    gh: &'s Github,
    shutdown: &'s GracefulShutdownHandle,
    hooks: Vec<Hook<N>>
}

pub struct Fetcher<'a, S: Strategy, N> {
    state: FetcherState<'a, N>,
    strategy: S,
}

impl<'s, N: NodeType> Fetcher<'s, strategy::DateWindow, N> {
    pub fn new_with_default_strategy(db: &'s DB, gh: &'s Github, shutdown: &'s GracefulShutdownHandle) -> Self {
        Fetcher::new(
            db,
            gh,
            shutdown,
            strategy::DateWindow {
                days_per_request: 1,
                ..Default::default()
            }
        )
    }
}

impl<'s, S: Strategy, N: NodeType> Fetcher<'s, S, N> {
    pub fn new(db: &'s DB, gh: &'s Github, shutdown: &'s GracefulShutdownHandle, strategy: S) -> Self {
        Fetcher {
            state: FetcherState {
                db,
                gh,
                shutdown,
                hooks: vec![],
            },
            strategy,
        }
    }

    pub fn add_node_hook<H: Fn(&N) + 'static>(&mut self, hook: H) {
        self.state.hooks.push(Box::new(hook))
    }

    pub fn fetch<'qa, 'qb>(&mut self, base_query: IncompleteQuery<'qa, 'qb, N>) -> Result<(), Error>
        where N: NodeType
    {
        self.strategy.prefetch_data(&self.state.db)?;
        self.strategy.execute(&self.state, base_query)?;
        Ok(())
    }
}


