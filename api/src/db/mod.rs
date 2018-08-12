use std::sync::{Arc, RwLock};
use std::path::Path;
use failure::Error;
use rocksdb::{DB, Options};

pub type KV = Arc<DB>;

pub fn open_and_init_db<L: Layout, P: AsRef<Path>>(path: P) -> Result<Arc<DB>, Error> {
    let db = L::open_cfs(path)?;
    Ok(Arc::new(db))
}

pub trait Layout {
    fn open_cfs<P: AsRef<Path>>(path: P) -> Result<DB, Error>;
}

pub mod cf {
    pub static REPOS: &str = "repo";
    pub static REPOS_META: &str = "repo-meta";
    pub static STATS: &str = "stats";
}

pub struct V1;

impl Layout for V1 {
    fn open_cfs<P: AsRef<Path>>(path: P) -> Result<DB, Error> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        let db = DB::open_cf(&opts, path, &[
            cf::REPOS,
            cf::STATS,
            cf::REPOS_META
        ])?;
        Ok(db)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use tempfile::Builder;
    use std::collections::HashSet;
    use super::*;

    fn new_path() -> PathBuf {
        let root = Builder::new().prefix("simple-db").tempdir().unwrap();
        fs::create_dir_all(root.path()).unwrap();
        root.path().to_owned()
    }

    fn open_db(path: &Path) -> Result<KV, Error> {
        open_and_init_db::<V1, _>(path)
    }

    fn new_db() -> Result<KV, Error> {
        let root = Builder::new().prefix("simple-db").tempdir().unwrap();
        fs::create_dir_all(root.path()).unwrap();
        open_and_init_db::<V1, _>(root.path())
    }


    #[test]
    fn open_close() {
        let db = new_db().unwrap();
        drop(db);
    }

    #[test]
    fn reopen() {
        let path = new_path();
        drop(open_db(&path));
        drop(open_db(&path));
        drop(open_db(&path));
        drop(open_db(&path));
    }

    #[test]
    fn columns_v1() {
        let path = new_path();
        drop(open_db(&path));

        let cfs = DB::list_cf(&Options::default(), &path).unwrap()
            .into_iter().collect::<HashSet<_>>();

        let valid_cfs = vec![
            "default".to_string(),
            cf::REPOS.to_string(),
            cf::REPOS_META.to_string(),
            cf::STATS.to_string(),
        ].into_iter().collect::<HashSet<_>>();

        assert_eq!(
            cfs, valid_cfs
        );
    }

    #[test]
    fn rw_default() {
        let db = new_db().unwrap();
        let key = b"rw_default";
        let value = b"rw_default_value";
        db.put(key, value).unwrap();
        assert_eq!(
            &db.get(key).unwrap().unwrap()[..],
            value
        );
    }

    #[test]
    fn rw_cf() {
        let db = new_db().unwrap();
        let key = b"rw_cf";
        let value = b"rw_cf_value";
        let cf = db.cf_handle(cf::REPOS).unwrap();
        db.put_cf(cf, key, value).unwrap();
        assert_eq!(
            &db.get_cf(cf, key).unwrap().unwrap()[..],
            value
        );
    }
}
