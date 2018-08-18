use std::path::{Path, PathBuf};
use rocksdb::IteratorMode;
use std::collections::HashMap;

struct Du

fn dump_json<P: AsRef<Path>>(base_path: P, create_dir: bool) {
    let db = db::open_and_init_db::<db::V1, _>(DB_PATH).unwrap();

    let date = chrono::Utc::today();
    let path = format!("./dump-{}/", date.format("%Y-%m-%d"));
    let path = PathBuf::from(path);
    std::fs::create_dir_all(&path).unwrap();

    dump_cf_json(&db, &path, "repositories.json", db::cf::REPOS);
    dump_cf_json(&db, &path, "meta.json", db::cf::REPOS_META);
    dump_cf_json(&db, &path, "stat.json", db::cf::STATS);
}

fn dump_cf_json(db: &db::KV, base_path: &Path, filename: &str, cf_name: &str) {
    let file = File::create(&base_path.join(filename)).unwrap();
    let cf = db.cf_handle(cf_name).unwrap();
    let data =  db.iterator_cf(cf, IteratorMode::Start).unwrap()
        .map(|(key, value)| (
            String::from_utf8(Vec::from(key)).unwrap(),
            String::from_utf8(Vec::from(value)).unwrap(),
        ))
        .collect::<HashMap<_, _>>();
    json::to_writer_pretty(file, &data).unwrap();
}