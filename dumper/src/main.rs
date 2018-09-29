// extern crate rustyrobot;

// #[macro_use]
// extern crate serde_derive;
// extern crate serde;
// extern crate serde_json as json;
// extern crate chrono;
// extern crate fern;
// #[macro_use]

// extern crate log;
// extern crate failure;
// extern crate ctrlc;

// use std::path::{Path, PathBuf};
// use std::collections::HashMap;
// use std::fs::{self, File};
// use failure::Error;
// use api::db;

// pub fn dump_json<P: AsRef<Path>>(db: &DB, base_path: P) -> Result<(), Error> {
//     let now = chrono::Utc::now();
//     let path = format!("dump-{}/", now.format("%Y-%m-%d-%H:%M:%S"));
//     let path = base_path.as_ref().join(path);
//     fs::create_dir_all(&path)?;

//     for cf in db::cf::CFS {
//         let mut filename = cf.to_lowercase();
//         filename.push_str(".json");
//         dump_cf_json(db, &path, &filename, cf)?;
//     }

//     Ok(())
// }

// fn dump_cf_json(db: &DB, base_path: &Path, filename: &str, cf_name: &str) -> Result<(), Error> {
//     let file = File::create(&base_path.join(filename))?;
//     let cf = db.cf_handle(cf_name).unwrap_or_else(|| panic!("Database column family {:?} doesn't exist", cf_name));
//     let data =  db.iterator_cf(cf, IteratorMode::Start)?
//         .map(|(key, value)| (
//             String::from_utf8(Vec::from(key)).unwrap(),
//             String::from_utf8(Vec::from(value)).unwrap(),
//         ))
//         .collect::<HashMap<_, _>>();
//     json::to_writer_pretty(file, &data)?;
//     Ok(())
// }


// use failure::Error;
// use std::io::Write;
// use std::fs::File;

// use std::time::{Instant, Duration as StdDuration};
// use chrono::Duration;

// fn init_fern() -> Result<(), Error> {
//     let (log_tx, log_rx) = std::sync::mpsc::channel::<String>();

//     std::thread::spawn(|| {
//         for line in log_rx {
//             drop(line)
//         }
//     });

//     fern::Dispatch::new()
//         .format(|out, message, record| {
//             out.finish(format_args!(
//                 "{}[{}][{}] {}",
//                 chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
//                 record.target(),
//                 record.level(),
//                 message
//             ))
//         })
//         .level_for("github_rustfmt_bot", log::LevelFilter::Debug)
//         .level_for("github_rustfmt_bot_api", log::LevelFilter::Debug)
//         .level(log::LevelFilter::Warn)
//         .chain(log_tx)
//         .chain(std::io::stdout())
//         .chain(File::create("bot.log").unwrap())
//         .apply()?;

//     info!("logger initialised");

//     Ok(())
// }

// use api::search::{search, query::{Query, Lang}};
// use api::db::KV;
// use types::Repository;
// use std::thread;
// use chrono::Utc;
// use fetcher::{Fetcher, strategy::DateWindow};
// use shutdown::{GracefulShutdown, GracefulShutdownHandle};

// fn main() {
//     init_fern().unwrap();
//     let db = db::open_and_init_db::<db::V1, _>(DB_PATH).unwrap();
//     let token = api::load_token().unwrap();

//     // Create graceful shutdown primitives
//     let shutdown = GracefulShutdown::new();

//     // Hook SIGINT signal
//     let sigint_shutdown = shutdown.clone();
//     ctrlc::set_handler(move || {
//         info!("got SIGINT (Ctrl-C) signal, shutting down");
//         sigint_shutdown.shutdown();
//     }).expect("couldn't register SIGINT handler");

//     // Start threads
//     let dumper = spawn_dumper_thread(db.clone(), shutdown.thread_handle());

//     // Wait until threads are finished
//     dumper.join().expect("dumper thread panicked");
// }

// use api::github::v4;
// use api::db::queue;
// use api::db::queue::Fork;
// use api::db::queue::QueueElement;
// use api::search::NodeType;

// fn spawn_dumper_thread(db: KV, shutdown: GracefulShutdownHandle) -> thread::JoinHandle<()> {
//     thread::spawn(move || {
//         let lock = shutdown.started("dumper");
//         dumper_thread_main(db, shutdown)
//     })
// }

// fn dumper_thread_main(db: KV, shutdown: GracefulShutdownHandle) {
//     let dump_period = Duration::hours(1);

//     let mut dump_time = Utc::now();// + dump_period;

//     while !shutdown.should_shutdown() {
//         if Utc::now() >= dump_time {
//             if let Err(e) = dump::dump_json(&db, DUMP_BASE_DIR) {
//                 error!("Failed to create dump: {}", e);
//             }
//             dump_time = Utc::now() + dump_period;
//         }
//         thread::sleep(StdDuration::from_secs(1));
//     }
// }

fn main() {

}
