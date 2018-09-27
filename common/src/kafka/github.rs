use super::Event;
use search::query::IncompleteQuery;
use types::Repository;
use chrono::DateTime;
use chrono::Utc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GithubEvent {
    RepositoryFetched(Repository),
}

impl Event for GithubEvent {}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GithubRequest {
    Fetch(IncompleteQuery),
}
