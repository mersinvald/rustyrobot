pub mod github;
pub mod util;

use serde::{Serialize, de::DeserializeOwned};

pub trait Event: Serialuze + DeserializeOwned {}
