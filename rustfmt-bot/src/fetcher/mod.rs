pub mod strategy;

pub use self::strategy::Strategy;

use std::sync::Arc;

use failure::Error;
use std::io::Write;
use std::fs::File;

use std::time::{Instant, Duration};

use api::db::KV;
use api::service::Handle;

use api::search::{search, NodeType, query::{Query, IncompleteQuery, Lang}};
use types::Repository;
use std::thread;
use chrono::Utc;
use api::db::stats;
use std::mem::discriminant;

use chrono::NaiveDate;

struct FetcherState {
    db: KV,
    token: String,
    gh: Handle,
}

pub struct Fetcher<S: Strategy> {
    data: FetcherState,
    strategy: S,
}

impl Fetcher<strategy::DateWindow> {
    pub fn new_with_defaults(db: KV, token: String, gh: Handle) -> Self {
        Fetcher::new(
            db,
            token,
            gh,
            strategy::DateWindow {
                days_per_request: 1,
                ..Default::default()
            }
        )
    }
}

impl<S: Strategy> Fetcher<S> {
    pub fn new(db: KV, token: String, gh: Handle, strategy: S) -> Self {
        Fetcher {
            data: FetcherState {
                db,
                token,
                gh,
            },
            strategy,
        }
    }

    pub fn fetch<'a, 'b, N>(&mut self, base_query: IncompleteQuery<'a, 'b, N>) -> Result<(), Error>
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
