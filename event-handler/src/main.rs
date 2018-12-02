extern crate rustyrobot;
extern crate rdkafka;
extern crate ctrlc;
extern crate failure;
extern crate log;
extern crate fern;
extern crate chrono;

use log::{info, error};
use failure::Error;

use rustyrobot::{
    kafka::{
        topic, group,
        Event,
        GithubRequest,
        util::handler::HandlingConsumer,
        util::producer::ThreadedProducer,
    },
    shutdown::{GracefulShutdown, GracefulShutdownHandle},
};

use std::thread;
use chrono::{Utc, Duration};
use std::time::Duration as StdDuration;

fn init_fern() -> Result<(), Error> {
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
        .level_for("event_handler", log::LevelFilter::Debug)
        .level_for("rustyrobot", log::LevelFilter::Debug)
        .level(log::LevelFilter::Warn)
        .chain(std::io::stdout())
        .apply()?;

    info!("logger initialised");

    Ok(())
}


fn main() {
    init_fern().unwrap();

    // Create graceful shutdown primitives
    let shutdown = GracefulShutdown::new();

    // Hook SIGINT signal
    let sigint_shutdown = shutdown.clone();
    ctrlc::set_handler(move || {
        info!("got SIGINT (Ctrl-C) signal, shutting down");
        sigint_shutdown.shutdown();
    }).expect("couldn't register SIGINT handler");

    // TODO
    // start_notification_fetch_loop(shutdown.thread_handle());
}

fn start_notification_fetch_loop(shutdown: GracefulShutdownHandle) -> Result<(), Error> {
    let fetch_period = Duration::minutes(5);
    let mut fetch_time = Utc::now();
    let producer = ThreadedProducer::new(topic::GITHUB_REQUEST, shutdown.clone())?;

    thread::spawn(move || {
        while !shutdown.should_shutdown() {
            if Utc::now() >= fetch_time {
                producer.send(GithubRequest::FetchNotifications)
                    .map_err(|e| error!("failed to send FetchEvent request: {}", e))
                    .ok();
                fetch_time = Utc::now() + fetch_period;
            }
            thread::sleep(StdDuration::from_secs(1));
        }
    });

    Ok(())
}

