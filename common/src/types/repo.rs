use search;
use chrono::{DateTime, Utc};
use json::{self, Value};
use failure::{err_msg, Error};

#[derive(Clone, Debug)]
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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryParent {
    pub name_with_owner: String,
    pub ssh_url: String,
    pub url: String,
}

impl search::NodeType for Repository {
    fn from_value(json: Value) -> Result<Self, Error> {
        let repo = v4::Repository::from_value(json.clone())
            .map(Repository::from)
            .or_else(|| v3::Repository::from_value(json)
                .map(Repository::from))?;
        Ok(repo)
    }
}

mod v4 {
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
                is_fork: v4.is_fork
            }
        }
    }
}

mod v3 {
    use failure::Error;
    use chrono::{DateTime, Utc};
    use json::{self, Value};
    use search::NodeType;

    #[derive(Clone, Debug, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Repository {
        pub id: i64,
        pub full_name: String,
        pub description: Option<String>,
        pub ssh_url: String,
        pub html_url: String,
        pub default_branch: String,
        pub created_at: DateTime<Utc>,
        // TODO
        pub parent: Option<super::RepositoryParent>,
        pub has_issues: bool,
        // TODO
        pub is_fork: bool,
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
                // TODO
                parent: v3.parent,
                has_issues_enabled: v3.has_issues,
                // TODO
                is_fork: v3.is_fork
            }
        }
    }
}

use std::str::FromStr;

// Fuck. This is mess. 
// TODO: Rewrite with enums and #[serde(flatten)] to parse both 
// v3 and v4 responces without suck dirty hacks
pub fn derive_fork(parent: &Repository, value: Value) -> Result<Repository, Error> {
    let value = value.as_object().ok_or_else(||
        err_msg("value is not an object")
    )?;

    let as_string = |value: &Value| value.as_str().map(ToOwned::to_owned).ok_or_else(||
        err_msg("value is not a string")
    );

    let mut fork = parent.clone();

    debug!("{:?}", value);

    fork.is_fork = true;
    fork.parent = Some(RepositoryParent {
        name_with_owner: parent.name_with_owner.clone(),
        ssh_url: parent.ssh_url.clone(),
        url: parent.url.clone(),
    });
    fork.id = value["id"].to_string();
    fork.name_with_owner = as_string(&value["full_name"])?;
    fork.description = value.get("description")
        .and_then(|v| if v.is_null() { None } else { Some(v) })
        .map(as_string).transpose()?;
    fork.url = as_string(&value["html_url"])?;
    fork.ssh_url = as_string(&value["ssh_url"])?;
    fork.default_branch_ref.name = as_string(&value["default_branch"])?;
    fork.has_issues_enabled = value["has_issues"].as_bool().ok_or_else(|| 
        err_msg("value is not boolean")
    )?;
    
    fork.created_at = DateTime::from_str(&as_string(&value["created_at"])?)?;

    debug!("derived fork: {:?}", fork);

    Ok(fork)
}

#[cfg(test)]
mod tests {
    #[test]
    fn deserialize_string() {

    }
}