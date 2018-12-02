use search;
use chrono::{DateTime, Utc};
use json::Value;
use failure::{err_msg, Error};
use std::collections::HashMap;
use log::error;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Repository {
    pub id: String,
    pub name_with_owner: String,
    pub description: Option<String>,
    pub ssh_url: String,
    pub url: String,
    pub default_branch: String,
    pub created_at: DateTime<Utc>,
    pub parent: Option<RepositoryParent>,
    pub has_issues_enabled: bool,
    pub is_fork: bool,
    pub stats: Option<Stats>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Stats {
    pub format: Option<FormatStats>,
    pub fix: Option<FixStats>,
    pub prs: Vec<PR>,
    pub aux: HashMap<String, Value>
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PR {
    pub title: String,
    pub number: i64,
    pub status: PRStatus
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PRStatus {
    Open,
    Merged,
    Closed,
}

impl PRStatus {
    pub fn from_str(from: &str) -> Option<Self> {
        match from.to_ascii_lowercase().as_ref() {
            "open" => Some(PRStatus::Open),
            "merged" => Some(PRStatus::Merged),
            "closed" => Some(PRStatus::Closed),
            other => {
                error!("couldn't parse {:?} as PRStatus", other);
                None
            }
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FormatStats {
    pub files_changed: u64,
    pub lines_added: u64,
    pub lines_removed: u64,
    pub branch: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FixStats {
    pub files_changed: u64,
    pub lines_added: u64,
    pub lines_removed: u64,
    pub err_cnt: u64,
    pub warn_cnt: u64,
    pub err_fixed: u64,
    pub warn_fixed: u64,
    pub fixed_lints: Vec<String>,
    pub branch: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RepositoryParent {
    pub name_with_owner: String,
    pub ssh_url: String,
    pub url: String,
}

impl search::NodeType for Repository {
    fn from_value(json: Value) -> Result<Self, Error> {
        let repo = match v4::Repository::from_value(json.clone()) {
            Ok(repo) => Repository::from(repo),
            Err(e) => {
                warn!("failed to deserialize repo as Github v4 API repo: {}", e);
                match v3::Repository::from_value(json) {
                    Ok(repo) => Repository::from(repo),
                    Err(e) => {
                        error!("failed to deserialize repo as Github v3 API repo: {}", e); 
                        bail!(err_msg("failed to deserialize Repository: not v4 nor v3 format"));
                    }
                }
            }
        };

        Ok(repo)
    }
}

pub mod v4 {
    use failure::Error;
    use chrono::{DateTime, Utc};
    use json::{self, Value};
    use search::NodeType;

    #[derive(Clone, Debug, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Repository {
        pub id: String,
        pub name_with_owner: String,
        pub description: Option<String>,
        pub ssh_url: String,
        pub url: String,
        pub default_branch_ref: BranchRef,
        pub created_at: DateTime<Utc>,
        pub parent: Option<super::RepositoryParent>,
        pub has_issues_enabled: bool,
        pub is_fork: bool,
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct BranchRef {
        pub name: String,
    }

    impl NodeType for Repository {
        fn from_value(json: Value) -> Result<Self, Error> {
            let repo = json::from_value(json)?;
            Ok(repo)
        }
    }

    impl From<Repository> for super::Repository {
        fn from(v4: Repository) -> super::Repository {
            super::Repository {
                id: v4.id,
                name_with_owner: v4.name_with_owner,
                description: v4.description,
                ssh_url: v4.ssh_url,
                url: v4.url,
                default_branch: v4.default_branch_ref.name,
                created_at: v4.created_at,
                parent: v4.parent,
                has_issues_enabled: v4.has_issues_enabled,
                is_fork: v4.is_fork,
                stats: None
            }
        }
    }
}

pub mod v3 {
    use failure::Error;
    use chrono::{DateTime, Utc};
    use json::{self, Value};
    use search::NodeType;

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct Repository {
        pub id: i64,
        pub full_name: String,
        pub description: Option<String>,
        pub ssh_url: String,
        pub html_url: String,
        pub default_branch: String,
        pub created_at: DateTime<Utc>,
        pub parent: Option<RepositoryParent>,
        pub has_issues: bool,
        pub fork: bool,
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct RepositoryParent {
        pub full_name: String,
        pub ssh_url: String,
        pub html_url: String,
    }


    impl NodeType for Repository {
        fn from_value(json: Value) -> Result<Self, Error> {
            let repo = json::from_value(json)?;
            Ok(repo)
        }
    }

    impl From<Repository> for super::Repository {
        fn from(v3: Repository) -> super::Repository {
            super::Repository {
                id: format!("{}", v3.id),
                name_with_owner: v3.full_name,
                description: v3.description,
                ssh_url: v3.ssh_url,
                url: v3.html_url,
                default_branch: v3.default_branch,
                created_at: v3.created_at,
                parent: v3.parent.map(|p| super::RepositoryParent {
                    name_with_owner: p.full_name,
                    ssh_url: p.ssh_url,
                    url: p.html_url,
                }),
                has_issues_enabled: v3.has_issues,
                is_fork: v3.fork,
                stats: None
            }
        }
    }
}
