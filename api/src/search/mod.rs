pub mod query;

use failure::Error;
use gh::client::Github;
use gh::query::Query as GqlQuery;
use gh::StatusCode;
use self::query::Query;
use chrono::{DateTime, Utc};
use std::fmt::Debug;

use json::Value;
use json;

use serde::Deserialize;

use github::RequestError;

pub trait NodeType {
    fn type_str() -> &'static str;
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    end_cursor: String,
    has_next_page: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchRef {
    id: String,
    name: String,
    prefix: String,
}

#[derive(Copy, Clone, Debug, Deserialize)]
enum RepositoryPermissions {
    ADMIN,
    READ,
    WRITE,
}

#[derive(Copy, Clone, Debug, Deserialize)]
enum RepositorySubscription {
    IGNORED,
    SUBSCRIBED,
    UNSUBSCRIBED,
}


#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepositoryFlags {
    has_issues_enabled: bool,
    is_archived: bool,
    is_fork: bool,
    is_locked: bool,
    is_private: bool,
    #[serde(rename = "viewerHasStarred")]
    starred: bool,
    #[serde(rename = "viewerPermission")]
    permission: RepositoryPermissions,
    #[serde(rename = "viewerSubscription")]
    subscribed: RepositorySubscription
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepositoryParent {
    name_with_owner: String,
    ssh_url: String,
    url: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepositoryMetadata {
    id: String,
    database_id: u64,
    created_at: DateTime<Utc>,
    default_branch_ref: BranchRef,
    disk_usage: u64,
    fork_count: u64,
    parent: Option<RepositoryParent>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Repository {
    name_with_owner: String,
    description: String,
    ssh_url: String,
    url: String,
    #[serde(flatten)]
    meta: RepositoryMetadata,
    #[serde(flatten)]
    flags: RepositoryFlags,
}

impl NodeType for Repository {
    fn type_str() -> &'static str {
        "REPOSITORY"
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult<N: NodeType> {
    page_info: PageInfo,
    repository_count: u64,
    nodes: Vec<N>,
}

static REPO_QUERY: &'static str = include_str!(
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/res/repo_query.gql"
    )
);

use error_chain_failure_interop::ResultExt;

pub fn search<N>(gh: &mut Github, query: Query<N>) -> Result<SearchResult<N>, Error>
    where N: NodeType + for<'de> Deserialize<'de> + Debug
{
    info!("performing search by query {:?}", query);

    // Build query
    let search_args = query.to_arg_list();
    let query = String::from(REPO_QUERY)
        .uglify()
        .replace("$ARGS$", &search_args);

    debug!("search query: {}", query);

    // Make a request
    let (headers, status, json) = gh.query::<Value>(
        &GqlQuery::new_raw(query)
    ).sync()?;

    debug!("search status: {}", status);
    let mut json = json.ok_or(RequestError::EmptyResponse)?;
    debug!("search response: {}", json);

    match status {
        StatusCode::Ok => (),
        status => bail!(RequestError::ResponseStatusNotOk { status })
    }

    // TODO: may panic
    let data = json["data"]["search"].take();
    let data = json::from_value(data)?;

    Ok(data)
}


trait Uglify {
    fn uglify(self) -> String;
}

use std::io::{Read, BufRead, Cursor};

impl Uglify for String {
    fn uglify(self) -> String {
        let mut buffer = String::with_capacity(self.len());
        let mut reader = Cursor::new(self);
        for line in reader.lines() {
            // shouldn't ever panic, no IO involved
            let line = line.unwrap();
            buffer.push_str(line.trim());
            buffer.push_str(" ");
        }
        buffer
    }
}
