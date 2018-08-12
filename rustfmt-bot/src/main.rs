extern crate github_rustfmt_bot_api as api;

#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_json as json;
extern crate rocksdb;
extern crate chrono;
extern crate fern;
#[macro_use]
extern crate log;
extern crate failure;


use chrono::{NaiveDate};

use api::db;
mod types;

static DB_PATH: &str = "./storage";

use failure::Error;
use std::io::Write;
use std::fs::File;

use std::time::{Instant, Duration};

fn init_fern() -> Result<(), Error> {
    let (log_tx, log_rx) = std::sync::mpsc::channel::<String>();

    std::thread::spawn(|| {
        for line in log_rx {
            drop(line)
        }
    });

    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{}[{}][{}] {}",
                chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
                record.target(),
                record.level(),
                message
            ))
        })
        .level_for("github_rustfmt_bot", log::LevelFilter::Debug)
        .level_for("github_rustfmt_bot_api", log::LevelFilter::Debug)
        .level(log::LevelFilter::Warn)
        .chain(log_tx)
        .chain(std::io::stdout())
        .chain(File::create("bot.log").unwrap())
        .apply()?;

    info!("logger initialised");

    Ok(())
}

use api::search::{search, query::{Query, Lang}};
use types::Repository;

fn main() {
    init_fern().unwrap();

    dump_json();
}

use std::path::{Path, PathBuf};
use rocksdb::IteratorMode;
use std::collections::HashMap;

fn dump_json() {
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

fn fetch() {
    let token = api::load_token().unwrap();

    let db = db::open_and_init_db::<db::V1, _>(DB_PATH).unwrap();

    let mut github = api::service::GithubService::new(&token);
    let mut handle = github.handle(Some("fetcher"));

    // Start the service
    github.start().unwrap();

    let meta = db.cf_handle(db::cf::REPOS_META).unwrap();
    let mut requests_count = 0;

    /*
    let mut date = db.get_cf(meta, b"last_date").unwrap()
        .and_then(|x| x.to_utf8().map(ToOwned::to_owned))
        .and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok())
        .unwrap_or(NaiveDate::from_ymd(2010, 01, 01));
        */
    let mut date = NaiveDate::from_ymd(2018, 9, 10);

    let mut start = Instant::now();

    // Date lop
    loop {

        let start_date = date.format("%Y-%m-%d").to_string();
        db.put_cf(meta, b"last_date", start_date.as_bytes()).unwrap();
        date = date.succ();
        let finish_date = date.format("%Y-%m-%d");
        date = date.succ();
        let created_query = format!("created:{}..{}", start_date, finish_date);
        info!("querying {}", created_query);

        // Pagination loop
        let mut page: Option<String> = None;

        let mut out_of_pages = false;
        while !out_of_pages {
            let query = Query::builder()
                .count(100)
                .lang(Lang::Rust)
                .raw_query(&created_query);

            let query = if let Some(page) = page.take() {
                query.after(page).build().unwrap()
            } else {
                query.build().unwrap()
            };

            requests_count += 1;
            let result = search::<Repository>(&mut handle, query);

            match result {
                Err(err) => {
                    panic!("couldn't make search query: {}\n{}", err, err.backtrace());
                },
                Ok(data) => {
                    let page_info = data.page_info;
                    let nodes = data.nodes;

                    for repo in nodes {
                        let json = json::to_string(&repo).unwrap();
                        let cf = db.cf_handle(db::cf::REPOS).unwrap();
                        db.put_cf(cf, repo.meta.id.as_bytes(), json.as_bytes()).unwrap();
                        info!("inserted repo {} info into db", repo.name_with_owner);
                    }

                    page = page_info.end_cursor;
                    if !page_info.has_next_page {
                        info!("reached EOF");
                        out_of_pages = true;
                    };
                }
            }

            info!("{} searches completed", requests_count);
//            if requests_count % 30 == 0 {
//                let elapsed = Instant::now() - start;
//                let timeout = Duration::from_secs(61) - elapsed;
//                info!("{} seconds timeout", timeout.as_secs());
//                std::thread::sleep(timeout);
//                start = Instant::now();
//            }
        }
    }
}
