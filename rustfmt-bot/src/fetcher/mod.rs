pub mod strategy;

pub use self::strategy::Strategy;

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

struct FetcherState<'s> {
    db: &'s DB,
    gh: &'s Github,
    shutdown: &'s GracefulShutdownHandle,
}

pub struct Fetcher<'a, S: Strategy> {
    data: FetcherState<'a>,
    strategy: S,
}

impl<'s> Fetcher<'s, strategy::DateWindow> {
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

impl<'s, S: Strategy> Fetcher<'s, S> {
    pub fn new(db: &'s DB, gh: &'s Github, shutdown: &'s GracefulShutdownHandle, strategy: S) -> Self {
        Fetcher {
            data: FetcherState {
                db,
                gh,
                shutdown
            },
            strategy,
        }
    }

    pub fn fetch<'qa, 'qb, N>(&mut self, base_query: IncompleteQuery<'qa, 'qb, N>) -> Result<(), Error>
        where N: NodeType
    {
        self.strategy.prefetch_data(&self.data.db)?;
        self.strategy.execute(&self.data, base_query)?;
        Ok(())
    }
}
