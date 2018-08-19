pub mod simple;
pub mod datewindow;

pub use self::simple::Simple;
pub use self::datewindow::DateWindow;

use rocksdb::DB;
use failure::Error;
use serde::Serialize;

use api::search::query::IncompleteQuery;
use api::search::NodeType;

use super::FetcherState;

pub trait Strategy {
    /// Fetch data from database before execution
    fn prefetch_data(&mut self, db: &DB) -> Result<(), Error> {
        // Doing nothing by default
        Ok(())
    }

    /// Run query using the strategy logic
    fn execute<'a, 'b, N>(&mut self, shared: &FetcherState, query: IncompleteQuery<'a, 'b, N>) -> Result<(), Error>
        where N: NodeType + Serialize;
}
