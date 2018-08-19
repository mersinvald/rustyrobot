use std::path::{Path, PathBuf};
use rocksdb::{DB, IteratorMode, Options};
use std::collections::HashMap;
use std::fs::{self, File};
use failure::Error;
use chrono;
use json;
use api::db;

pub fn dump_json<P: AsRef<Path>>(db: &DB, base_path: P) -> Result<(), Error> {
    let now = chrono::Utc::now();
    let path = format!("dump-{}/", now.format("%Y-%m-%d-%H:%M:%S"));
    let path = base_path.as_ref().join(path);
    fs::create_dir_all(&path)?;

    for cf in db::cf::CFS {
        let mut filename = cf.to_lowercase();
        filename.push_str(".json");
        dump_cf_json(db, &path, &filename, cf)?;
    }

    Ok(())
}

fn dump_cf_json(db: &DB, base_path: &Path, filename: &str, cf_name: &str) -> Result<(), Error> {
    let file = File::create(&base_path.join(filename))?;
    let cf = db.cf_handle(cf_name).unwrap_or_else(|| panic!("Database column family {:?} doesn't exist", cf_name));
    let data =  db.iterator_cf(cf, IteratorMode::Start)?
        .map(|(key, value)| (
            String::from_utf8(Vec::from(key)).unwrap(),
            String::from_utf8(Vec::from(value)).unwrap(),
        ))
        .collect::<HashMap<_, _>>();
    json::to_writer_pretty(file, &data)?;
    Ok(())
}
