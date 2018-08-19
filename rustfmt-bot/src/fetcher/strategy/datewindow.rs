use super::Strategy;

use rocksdb::DB;
use failure::Error;
use chrono::NaiveDate;
use chrono::Utc;
use chrono::Duration;
use json;

use api::search::search;
use api::search::query::IncompleteQuery;
use api::db;
use api::db::stats;
use api::search::NodeType;

use fetcher::FetcherState;

#[derive(Default)]
pub struct DateWindow {
    /// Date step length
    pub days_per_request: u64,

    /// The repo creation date to begin searching from
    /// If None -- the last processed date would be fetched from DB
    /// If DB entry is empty -- Utc::today would be used
    pub start_date: Option<NaiveDate>,

    /// The repo creation date to finish the search
    /// If None -- Utc::today() is used
    pub end_date: Option<NaiveDate>,

    pub state: DateWindowState,
}

pub struct DateWindowState {
    date: NaiveDate,
}

impl Default for DateWindowState {
    fn default() -> Self {
        DateWindowState {
            date: Utc::today().naive_utc()
        }
    }
}

impl Strategy for DateWindow {
    /// Fetch data from database before execution
    fn prefetch_data(&mut self, db: &DB) -> Result<(), Error> {
        let meta = db.cf_handle(db::cf::REPOS_META).unwrap();

        self.start_date = match self.start_date.take() {
            Some(date) => Some(date),
            None => db.get_cf(meta, b"last_date")?
                .and_then(|x| x.to_utf8().map(ToOwned::to_owned))
                .and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok())
                .or_else(|| Some(Utc::today().naive_utc()))
        };

        self.end_date = self.end_date.take()
            .or_else(|| Some(Utc::today().naive_utc()));

        Ok(())
    }

    /// Run query using the strategy logic
    fn execute<'a, 'b, N>(&mut self, shared: &FetcherState, query: IncompleteQuery<'a, 'b, N>) -> Result<(), Error>
        where N: NodeType
    {
        let meta = shared.db.cf_handle(db::cf::REPOS_META).unwrap();

        // At this moment start_date MUST be Some
        self.state.date = self.start_date.unwrap();
        let step = Duration::days(self.days_per_request as i64);

        while self.state.date <= Utc::today().naive_utc() {

            let window_start = self.state.date.format("%Y-%m-%d").to_string();
            let window_end = self.state.date + step;

            shared.db.put_cf(meta, b"last_date", window_start.as_bytes())?;
            self.state.date = window_end.succ();

            let date_query_segment = format!("created:{}..{}", window_start, window_end.format("%Y-%m-%d"));
            info!("querying {}", date_query_segment);

            let query = query.clone().raw_query(date_query_segment);

            // Reuse simple strategy for making single request
            super::Simple.execute(shared, query)?;
        }

        Ok(())
    }
}
