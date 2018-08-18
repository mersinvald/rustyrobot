use json;
use rocksdb::DB;
use failure::Error;
use std::str;
use serde::{
    Serialize,
    de::DeserializeOwned
};

pub fn get_stat_entry<D>(db: &DB, key: &str) -> Result<Option<D>, Error>
    where D: DeserializeOwned
{
    let cf = db.cf_handle(super::cf::STATS).unwrap();
    if let Some(value) = db.get_cf(cf, key.as_bytes())? {
        let value = json::from_str(str::from_utf8(&value[..])?)?;
        Ok(Some(value))
    } else {
        Ok(None)
    }
}

pub fn put_stat_entry<S>(db: &DB, key: &str, value: S) -> Result<(), Error>
    where S: Serialize
{
    let cf = db.cf_handle(super::cf::STATS).unwrap();
    db.put_cf(cf, key.as_bytes(), json::to_string(&value)?.as_bytes())?;
    Ok(())
}

pub fn increment_stat_counter(db: &DB, key: &str) -> Result<(), Error> {
    let old: i64 = get_stat_entry(db, key)?.unwrap_or(0);
    put_stat_entry(db, key, old + 1)?;
    Ok(())
}
