use api::search;
use chrono::{DateTime, Utc};
use api::db;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Repository {
    pub name_with_owner: String,
    pub description: Option<String>,
    pub ssh_url: String,
    pub url: String,
    #[serde(flatten)]
    pub meta: RepositoryMetadata,
    #[serde(flatten)]
    pub flags: RepositoryFlags,
}

impl search::NodeType for Repository {
    fn id(&self) -> &str {
        &self.meta.id
    }

    fn column_family() -> &'static str {
        db::cf::REPOS
    }

    fn type_str() -> &'static str {
        "REPOSITORY"
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchRef {
    pub id: String,
    pub name: String,
    pub prefix: String,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub enum RepositoryPermissions {
    ADMIN,
    READ,
    WRITE,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub enum RepositorySubscription {
    IGNORED,
    SUBSCRIBED,
    UNSUBSCRIBED,
}


#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryFlags {
    pub has_issues_enabled: bool,
    pub is_archived: bool,
    pub is_fork: bool,
    pub is_locked: bool,
    pub is_private: bool,
    #[serde(rename = "viewerHasStarred")]
    pub starred: bool,
    #[serde(rename = "viewerPermission")]
    pub permission: RepositoryPermissions,
    #[serde(rename = "viewerSubscription")]
    pub subscribed: RepositorySubscription
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryParent {
    pub name_with_owner: String,
    pub ssh_url: String,
    pub url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryMetadata {
    pub id: String,
    pub database_id: u64,
    pub created_at: DateTime<Utc>,
    pub default_branch_ref: BranchRef,
    pub disk_usage: u64,
    pub fork_count: u64,
    pub parent: Option<RepositoryParent>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryInternalBotMeta {

}

