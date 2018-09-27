use super::Strategy;

use failure::Error;
use chrono::NaiveDate;
use chrono::Utc;
use chrono::Duration;
use json;

use rustyrobot::{
    search::{
        search,
        query::IncompleteQuery,
    },
};

use fetcher::FetcherState;

#[derive(Default, Clone)]
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

#[derive(Clone)]
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
    /// Run query using the strategy logic
    fn execute(&mut self, shared: &mut FetcherState, query: IncompleteQuery) -> Result<(), Error> {
        let start_date = if let Some(start_date) = self.start_date {
            start_date
        } else {
            let date: String = shared.state.get_or_default("last_date");
            NaiveDate::parse_from_str(&date, "%Y-%m-%d")
                .unwrap_or_else(|e| {
                    error!("failed to parse last_date: {}", e);
                    error!("using Utc::today()");
                    Utc::today().naive_utc()
                })
        };

        self.state.date = start_date;
        let step = Duration::days(self.days_per_request as i64);

        while self.state.date <= Utc::today().naive_utc() && !shared.shutdown.should_shutdown() {

            let window_start = self.state.date.format("%Y-%m-%d").to_string();
            let window_end = self.state.date + step;
            shared.state.set("last_date", window_start.clone());
            shared.state.sync()?;
            self.state.date = window_end.succ();

            let date_query_segment = format!("created:{}..{}", window_start, window_end.format("%Y-%m-%d"));
            info!("requesting {}", date_query_segment);
            let query = query.clone().raw_query(date_query_segment);

            // Reuse simple strategy for making single request
            super::Simple.execute(shared, query)?;
        }

        Ok(())
    }
}
