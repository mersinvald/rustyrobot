use failure::Error;

use rustyrobot::kafka::GithubRequest;
use rustyrobot::search::query::IncompleteQuery;

use super::Strategy;
use fetcher::FetcherState;

pub struct Simple;

impl Strategy for Simple {
    /// Run query using the strategy logic
    fn execute(&mut self, shared: &mut FetcherState, query: IncompleteQuery) -> Result<(), Error> {
        shared.producer.send(GithubRequest::Fetch(query))?;
        Ok(())
    }
}
