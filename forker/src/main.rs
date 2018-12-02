extern crate ctrlc;
extern crate failure;
extern crate rdkafka;
extern crate rustyrobot;
#[macro_use]
extern crate log;
extern crate chrono;
extern crate fern;

use failure::Error;

use rustyrobot::{
    kafka::{group, topic, util::handler::HandlingConsumer, Event, GithubRequest},
    shutdown::GracefulShutdown,
};

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
        .level_for("forker", log::LevelFilter::Debug)
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
    })
    .expect("couldn't register SIGINT handler");

    HandlingConsumer::builder()
        .subscribe(topic::EVENT)
        .respond_to(topic::GITHUB_REQUEST)
        .group(group::FORKER)
        .handler(|event, callback| {
            match event {
                Event::RepositoryFetched(repo) => callback(GithubRequest::Fork(repo)),
                _ => (),
            }
            Ok(())
        })
        .build()
        .expect("failed to build handler")
        .start(shutdown.thread_handle())
        .expect("github service failed");
}
