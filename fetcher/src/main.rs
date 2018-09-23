extern crate rustyrobot_common as api;

#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_json as json;
extern crate chrono;
extern crate fern;
#[macro_use]
extern crate log;
extern crate failure;
extern crate ctrlc;

mod strategy;
mod fetcher;


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
        .level_for("fetcher", log::LevelFilter::Debug)
        .level_for("rustyrobot_common", log::LevelFilter::Debug)
        .level(log::LevelFilter::Warn)
        .chain(log_tx)
        .chain(std::io::stdout())
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

    // Create graceful shutdown primitives
    let shutdown = GracefulShutdown::new();

    // Hook SIGINT signal
    let sigint_shutdown = shutdown.clone();
    ctrlc::set_handler(move || {
        info!("got SIGINT (Ctrl-C) signal, shutting down");
        sigint_shutdown.shutdown();
    }).expect("couldn't register SIGINT handler");

    // Start threads
    let fetcher = spawn_fetcher_thread(db.clone(), token, shutdown.thread_handle());

    // Wait until threads are finished
    fetcher.join().expect("fetcher thread panicked");
}

use api::github::v4;

fn spawn_fetcher_thread(db: KV, token: String, shutdown: GracefulShutdownHandle) -> thread::JoinHandle<()> {
    thread::spawn(|| {
        let lock = shutdown.started("fetcher");
        fetcher_thread_main(db, token, shutdown);
    })
}

use api::db::queue;
use api::db::queue::Fork;
use api::db::queue::QueueElement;
use api::search::NodeType;

fn fetcher_thread_main(db: KV, token: String, shutdown: GracefulShutdownHandle) {
    let gh = v4::Github::new(db.clone(), &token)
        .expect("failed to create github client");

    let query = Query::builder()
        .lang(Lang::Rust)
        .owner("mersinvald")
        .count(100);

    let mut strategy = DateWindow {
        days_per_request: 1,
        start_date: Some(NaiveDate::from_ymd(2018, 8, 10)),
        ..Default::default()
    };

    let fetch_period = Duration::hours(1);
    let mut fetch_time = Utc::now();

    while !shutdown.should_shutdown() {
        if Utc::now() >= fetch_time {
            let mut fetcher = Fetcher::new(&db, &gh, &shutdown, strategy.clone());

            // Resetting start_date in strategy so we won't start over in next iteration
            strategy.start_date = None;

            // Registering hook in order to fill the forking queue
            let hook_db = db.clone();
            fetcher.add_node_hook(move |repo: &Repository| {
                if let Err(err) = queue::enqueue(&hook_db, &Fork::empty_with_id(repo.id())) {
                    warn!("failed to enqueue repo {}: {}", repo.name_with_owner, err);
                }
            });

            // If that fails, fetcher will start from last successful data
            if let Err(e) = fetcher.fetch(query.clone()) {
                error!("failed to fetch repositories: {}", e);

            } else {
                // If success, moving fetch_time one period into the future,
                // next iteration will start from Utc::today()
                fetch_time = Utc::now() + fetch_period;
            }


        }
        thread::sleep(StdDuration::from_secs(1));
    }
}
