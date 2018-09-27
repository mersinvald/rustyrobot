use search;
use chrono::{DateTime, Utc};
use json::{self, Value};
use failure::{err_msg, Error};

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
    pub parent: Option<RepositoryParent>,
    pub has_issues_enabled: bool,
    pub is_fork: bool,
}

impl search::NodeType for Repository {}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchRef {
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryParent {
    pub name_with_owner: String,
    pub ssh_url: String,
    pub url: String,
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