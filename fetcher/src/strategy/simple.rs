use failure::Error;
use serde::Serialize;
use json;

use api::search::search;
use api::search::NodeType;
use api::search::query::IncompleteQuery;
use api::db;
use api::db::stats;
use api::db::queue;
use api::db::queue::QueueElement;


use super::Strategy;
use fetcher::FetcherState;

pub struct Simple;

impl Strategy for Simple {
    /// Run query using the strategy logic
    fn execute<'a, 'b, N>(&mut self, shared: &FetcherState<N>, query: IncompleteQuery<'a, 'b, N>) -> Result<(), Error>
        where N: NodeType + Serialize
    {
        // Pre-format for stats db column
        let node_typename = N::type_str().to_lowercase();
        let node_stat_key = format!("fetcher_{}_inserted", node_typename);

        // Aliases to shared state
        let db = shared.db;
        let gh = shared.gh;

        // Current page
        let mut page = None;
        let mut out_of_pages = false;

        while !out_of_pages && !shared.shutdown.should_shutdown() {
            let query = query.clone();

            let query = if let Some(page) = page.take() {
                query.after(page).build()?
            } else {
                query.build()?
            };

            let data = search(&gh, query)?;

            let page_info = data.page_info;
            let nodes = data.nodes;

            for node in nodes {
                let json = json::to_string(&node)?;
                let cf = db.cf_handle(N::column_family()).unwrap();
                db.put_cf(cf, node.id().as_bytes(), json.as_bytes())?;
                stats::increment_stat_counter(db, &node_stat_key)?;
                debug!("inserted {} {} info into db", node_typename, node.id());

                // call hooks on node
                for hook in &shared.hooks {
                    hook(&node)
                }
            }

            page = page_info.end_cursor;
            if !page_info.has_next_page {
                debug!("reached EOF");
                out_of_pages = true;
            };
        }

        Ok(())
    }
}
