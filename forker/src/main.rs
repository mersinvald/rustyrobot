extern crate rustyrobot;
extern crate rdkafka;
extern crate ctrlc;
extern crate failure;
#[macro_use]
extern crate log;
#[macro_use]
extern crate fern;
extern crate chrono;

use failure::Error;

use rustyrobot::{
    kafka::{
        topic, group,
        github::{GithubRequest, GithubEvent},
        util::{
            producer::ThreadedProducer,
            handler::HandlingConsumer,
            state::StateHandler,
        }
    },
    github::v4::Github as GithubV4,
    github::v3::Github as GithubV3,
    github::utils::load_token,
    types::Repository,
    search::{
        search,
        query::SearchFor,
        query::{Lang, Query, IncompleteQuery},
    },
    shutdown::{GracefulShutdown, GracefulShutdownHandle},
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
    }).expect("couldn't register SIGINT handler");

    HandlingConsumer::builder()
        .pool_size(4)
        .subscribe(topic::GITHUB_EVENT)
        .respond_to(topic::GITHUB_REQUEST)
        .group(group::FORKER)
        .handler(|event, callback| {
            match event {
                GithubEvent::RepositoryFetched(repo) => callback(GithubRequest::Fork(repo)),
                _ => (),
            }
            Ok(())
        })
        .build()
        .expect("failed to build handler")
        .start(shutdown.thread_handle())
        .expect("github service failed");
}
