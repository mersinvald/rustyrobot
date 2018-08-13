use rocksdb::DB;
use failure::Error;
use serde::{
    Serialize,
    de::DeserializeOwned
};

pub fn get_stat_entry<D>(db: &DB, key: &str) -> Result<D, Error>
    where D: DeserializeOwned
{

}

pub fn put_stat_entry<S>(db: &DB, key: &str, value: S) -> Result<(), Error>
    where S: Serialize
{
    let cf = db.cf_handle(super::cf::STATS)?;
    let db
}

/// # Panics
/// panics if value is not an integer
pub fn increment_stat_counter(db: &DB, key: &str) {

}
