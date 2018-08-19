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
use api::db::KV;
use types::Repository;
use std::thread;
use chrono::Utc;
use fetcher::{Fetcher, strategy::DateWindow};
use shutdown::{GracefulShutdown, GracefulShutdownHandle};

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
    }).expect("couldn't register SIGINT handler");

    // Start threads
    let fetcher = spawn_fetcher_thread(db.clone(), github.handle(Some("fetcher")), shutdown.thread_handle());
    let dumper = spawn_dumper_thread(db.clone(), shutdown.thread_handle());

    // Start the service
    github.start().unwrap();

    // Wait until threads are finished
    fetcher.join().expect("fetcher thread panicked");
    dumper.join().expect("dumper thread panicked");
}

fn spawn_fetcher_thread(db: KV, gh: api::service::Handle, shutdown: GracefulShutdownHandle) -> thread::JoinHandle<()> {
    thread::spawn(|| {
        let lock = shutdown.started("fetcher");
        fetcher_thread_main(db, gh, shutdown);
    })
}

fn fetcher_thread_main(db: KV, gh: api::service::Handle, shutdown: GracefulShutdownHandle) {
    let query = Query::builder()
        .lang(Lang::Rust)
        .count(100);

    let mut strategy = DateWindow {
        days_per_request: 1,
        ..Default::default()
    };

    let fetch_period = Duration::hours(1);
    let mut fetch_time = Utc::now();

    while !shutdown.should_shutdown() {
        if Utc::now() >= fetch_time {
            if let Err(e) = Fetcher::new(&db, &gh, &shutdown, strategy.clone())
                .fetch::<Repository>(query.clone())
            {
                error!("failed to fetch repositories: {}", e);
            }
            fetch_time = Utc::now() + fetch_period;
        }
        thread::sleep(StdDuration::from_secs(1));
    }
}

fn spawn_dumper_thread(db: KV, shutdown: GracefulShutdownHandle) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let lock = shutdown.started("dumper");
        dumper_thread_main(db, shutdown)
    })
}

fn dumper_thread_main(db: KV, shutdown: GracefulShutdownHandle) {
    let dump_period = Duration::hours(1);

    let mut dump_time = Utc::now() + dump_period;

    while !shutdown.should_shutdown() {
        if Utc::now() >= dump_time {
            if let Err(e) = dump::dump_json(&db, DUMP_BASE_DIR) {
                error!("Failed to create dump: {}", e);
            }
            dump_time = Utc::now() + dump_period;
        }
        thread::sleep(StdDuration::from_secs(1));
    }
}
