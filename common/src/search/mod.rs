pub mod query;

use self::query::Query;
use failure::Error;
use github::v4::Github;
use std::fmt::Debug;

use json;
use json::Value;

use serde::de::DeserializeOwned;
use serde::Serialize;

pub trait NodeType: Serialize + DeserializeOwned + Clone + Debug {
    fn from_value(json: Value) -> Result<Self, Error>;
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    pub end_cursor: Option<String>,
    pub has_next_page: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult<N> {
    pub page_info: PageInfo,
    pub repository_count: u64,
    pub nodes: Vec<N>,
}

static REPO_QUERY: &'static str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/res/repo_query.gql"));

pub fn search<N>(gh: &Github, query: Query) -> Result<SearchResult<N>, Error>
where
    N: NodeType,
{
    info!("performing search by {:?}", query);

    // Build query
    let search_args = query.to_arg_list();
    let query = String::from(REPO_QUERY)
        .uglify()
        .replace("$ARGS$", &search_args);

    trace!("search {}", query);

    // Make a request
    let mut json: Value = gh.query("search", query)?;

    // TODO: may panic
    let data = json["data"]["search"].take();
    let data = json::from_value(data)?;

    Ok(data)
}

trait Uglify {
    fn uglify(self) -> String;
}

use std::io::{BufRead, Cursor};

impl Uglify for String {
    fn uglify(self) -> String {
        let mut buffer = String::with_capacity(self.len());
        let reader = Cursor::new(self);
        for line in reader.lines() {
            // shouldn't ever panic, no IO involved
            let line = line.unwrap();
            buffer.push_str(line.trim());
            buffer.push_str(" ");
        }
        buffer
    }
}
