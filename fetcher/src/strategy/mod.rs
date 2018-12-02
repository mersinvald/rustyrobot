pub mod datewindow;
pub mod simple;

pub use self::datewindow::DateWindow;
pub use self::simple::Simple;

use failure::Error;

use rustyrobot::search::query::IncompleteQuery;

use fetcher::FetcherState;

pub trait Strategy {
    /// Run query using the strategy logic
    fn execute<'a>(
        &mut self,
        shared: &mut FetcherState<'a>,
        query: IncompleteQuery,
    ) -> Result<(), Error>;
}
