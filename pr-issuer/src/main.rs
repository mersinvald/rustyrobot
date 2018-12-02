extern crate ctrlc;
extern crate rdkafka;
extern crate rustyrobot;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate log;
#[macro_use]
extern crate fern;
extern crate chrono;
extern crate github_rs as github_v3;
#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_json as json;

use failure::{err_msg, Error};
use std::sync::{Arc, Mutex};

use rustyrobot::{
    github::utils::load_token,
    github::v3::Github as GithubV3,
    github::v4::Github as GithubV4,
    kafka::{
        group, topic,
        util::{
            handler::{HandlerError, HandlingConsumer},
            state::StateHandler,
        },
        Event, GithubRequest,
    },
    search::{query::IncompleteQuery, query::SearchFor, search},
    shutdown::{GracefulShutdown, GracefulShutdownHandle},
    types::Repository,
};

use rdkafka::{
    producer::{DefaultProducerContext, ThreadedProducer},
    ClientConfig,
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
        .level_for("github", log::LevelFilter::Debug)
        .level_for("pr_issuer", log::LevelFilter::Debug)
        .level(log::LevelFilter::Warn)
        .chain(std::io::stdout())
        .apply()?;

    info!("logger initialised");

    Ok(())
}

const PR_MSG: &'static str = include_str!("../pr_message.md");

fn main() {
    init_fern().expect("failed to setup logger");

    let shutdown = GracefulShutdown::new();
    let shutdown_handle = shutdown.thread_handle();

    ctrlc::set_handler(move || {
        info!("received Ctrl-C, shutting down");
        shutdown.shutdown()
    })
    .unwrap();

    HandlingConsumer::builder()
        .subscribe(topic::EVENT)
        .respond_to(topic::GITHUB_REQUEST)
        .group(group::PR_ISSUER)
        .handler(|event, callback| {
            match event {
                Event::RepositoryFormatted(repo) => {
                    let branch = {
                        let stats = repo.stats.as_ref().ok_or(HandlerError::Internal {
                            error: err_msg("stats are empty after the formatting stage"),
                        })?;

                        let fmt_stats = stats.format.as_ref().ok_or(HandlerError::Internal {
                            error: err_msg("formatting stats are empty after the formatting stage"),
                        })?;

                        fmt_stats.branch.clone()
                    };

                    callback(GithubRequest::CreatePR {
                        repo,
                        branch,
                        title: "Formatting Suggestions from RustyRobot".to_string(),
                        message: PR_MSG.to_string(),
                    })
                }
                _ => (),
            }
            Ok(())
        })
        .build()
        .expect("failed to build handler")
        .start(shutdown_handle)
        .expect("formatter service failed");
}
