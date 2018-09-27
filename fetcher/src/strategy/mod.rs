pub mod simple;
pub mod datewindow;

pub use self::simple::Simple;
pub use self::datewindow::DateWindow;

use failure::Error;
use serde::Serialize;

use rustyrobot::search::query::IncompleteQuery;

use fetcher::FetcherState;

pub trait Strategy {
    /// Run query using the strategy logic
    fn execute<'a>(&mut self, shared: &mut FetcherState<'a>, query: IncompleteQuery) -> Result<(), Error>;
}
