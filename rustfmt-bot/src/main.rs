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
extern crate ctrlc;


use chrono::{NaiveDate};

use api::db;
mod types;
mod fetcher;
mod dump;
mod shutdown;

static DB_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/storage/");
static DUMP_BASE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/dumps/");

use failure::Error;
use std::io::Write;
use std::fs::File;

use std::time::{Instant, Duration as StdDuration};
use chrono::Duration;

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
use std::thread;
use chrono::Utc;
use fetcher::{Fetcher, strategy::DateWindow};
use shutdown::GracefulShutdown;

fn main() {
    init_fern().unwrap();
    let db = db::open_and_init_db::<db::V1, _>(DB_PATH).unwrap();
    let token = api::load_token().unwrap();
    let mut github = api::service::GithubService::new(db.clone(), &token);

    // Create graceful shutdown primitives
    let shutdown = GracefulShutdown::new();

    // Hook SIGINT signal
    let sigint_shutdown = shutdown.clone();
    ctrlc::set_handler(move || {
        info!("got SIGINT (Ctrl-C) signal, shutting down");
        sigint_shutdown.shutdown();
    }).expect("Couldn't register SIGINT handler");

    // Create fetcher thread
    let github_handle = github.handle(Some("fetcher"));
    let shutdown_handle = shutdown.thread_handle();
    let fetcher_db = db.clone();
    let fetcher = thread::spawn(move || {
        let lock = shutdown_handle.started("fetcher");

        let query = Query::builder()
            .lang(Lang::Rust)
            .count(100);

        let strategy = DateWindow {
            days_per_request: 1,
            start_date: Some(NaiveDate::from_ymd(2016, 1, 1)),
            ..Default::default()
        };

        Fetcher::new(fetcher_db, token, github_handle, shutdown_handle, strategy)
            .fetch::<Repository>(query)
    });

    // Create dumper thread
    let dumper_db = db.clone();
    let shutdown_handle = shutdown.thread_handle();
    let dumper = thread::spawn(move || {
        let lock = shutdown_handle.started("dumper");
        let dump_period = Duration::hours(1);

        let mut dump_time = Utc::now() + dump_period;

        while !shutdown_handle.should_shutdown() {
            if Utc::now() >= dump_time {
                if let Err(e) = dump::dump_json(&dumper_db, DUMP_BASE_DIR) {
                    error!("Failed to create dump: {}", e);
                }
                dump_time = Utc::now() + dump_period;
            }
            thread::sleep(StdDuration::from_secs(1));
        }
    });

    // Start the service
    github.start().unwrap();

    // Wait until threads are finished
    while shutdown.threads_running() != 0 {
        thread::sleep(StdDuration::from_secs(5));
    }
}
