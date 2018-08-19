pub mod strategy;

pub use self::strategy::Strategy;

use std::sync::Arc;

use failure::Error;
use std::io::Write;
use std::fs::File;

use std::time::{Instant, Duration};

use rocksdb::DB;
use api::db::KV;
use api::service::Handle;

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
    gh: &'s Handle,
    shutdown: &'s GracefulShutdownHandle,
}

pub struct Fetcher<'a, S: Strategy> {
    data: FetcherState<'a>,
    strategy: S,
}

impl<'s> Fetcher<'s, strategy::DateWindow> {
    pub fn new_with_default_strategy(db: &'s DB, gh: &'s Handle, shutdown: &'s GracefulShutdownHandle) -> Self {
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
    pub fn new(db: &'s DB, gh: &'s Handle, shutdown: &'s GracefulShutdownHandle, strategy: S) -> Self {
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

/*
fn fetch() {
    let meta = db.cf_handle(db::cf::REPOS_META).unwrap();
    let mut requests_count = 0;

    /*
    let mut date = db.get_cf(meta, b"last_date").unwrap()
        .and_then(|x| x.to_utf8().map(ToOwned::to_owned))
        .and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok())
        .unwrap_or(NaiveDate::from_ymd(2010, 01, 01));
        */
    let mut date = NaiveDate::from_ymd(2018, 9, 10);

    let mut start = Instant::now();

    // Date lop
    loop {

        let start_date = date.format("%Y-%m-%d").to_string();
        db.put_cf(meta, b"last_date", start_date.as_bytes()).unwrap();
        date = date.succ();
        let finish_date = date.format("%Y-%m-%d");
        date = date.succ();
        let created_query = format!("created:{}..{}", start_date, finish_date);
        info!("querying {}", created_query);

        fetch_query()
    }
}
*/
