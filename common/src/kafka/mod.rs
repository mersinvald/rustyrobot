pub mod github;
pub mod util;

use serde::{Serialize, de::DeserializeOwned};

pub trait Event: Serialize + DeserializeOwned {}

pub mod topic {
    pub const GITHUB_REQUEST: &str = "rustyrobot.github.request";
    pub const GITHUB_EVENT: &str = "rustyrobot.github.event";
    pub const GITHUB_STATE: &str = "rustyrobot.github.state";
}

pub mod group {
    pub const GITHUB: &str = "rustyrobot.github";
}
