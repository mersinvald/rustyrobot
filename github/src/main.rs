extern crate rustyrobot;
extern crate rdkafka;
extern crate ctrlc;
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

use std::sync::{Arc, Mutex};
use failure::Error;

use rustyrobot::{
    kafka::{
        topic, group,
        github::{GithubRequest, GithubEvent},
        util::{
            handler::{HandlingConsumer, HandlerError},
            state::StateHandler,
        }
    },
    github::v4::Github as GithubV4,
    github::v3::Github as GithubV3,
    github::utils::load_token,
    types::{Repository, derive_fork},
    search::{
        search,
        query::SearchFor,
        query::IncompleteQuery,
    },
    shutdown::{GracefulShutdown, GracefulShutdownHandle},
};

use rdkafka::{
    ClientConfig,
    producer::{
        ThreadedProducer,
        DefaultProducerContext
    }
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
        .level_for("rustyrobot", log::LevelFilter::Debug)
        .level(log::LevelFilter::Warn)
        .chain(std::io::stdout())
        .apply()?;

    info!("logger initialised");

    Ok(())
}

fn main() {
    init_fern().expect("failed to setup logger");

    let shutdown = GracefulShutdown::new();
    let shutdown_handle = shutdown.thread_handle();

    ctrlc::set_handler(move || {
        info!("received Ctrl-C, shutting down");
        shutdown.shutdown()
    }).unwrap();

    let mut state = StateHandler::new(topic::GITHUB_STATE)
        .expect("failed to open state");
    state.restore().expect("failed to restore state");
    let state = Arc::new(Mutex::new(state));

    let token = load_token()
        .expect("failed to load token (set GITHUB_TOKEN env)");

    let github_v3 = GithubV3::new(&token).expect("failed to create GitHub V3 API instance");
    let github_v4 = GithubV4::new(&token).expect("failed to create GitHub V4 API instance");

    let handler = {
        let shutdown_handle = shutdown_handle.clone();
        move |msg, callback: &mut dyn FnMut(GithubEvent)| {
            let increment_stat_counter = |key| {
                let mut state = state.lock().unwrap();
                state.increment(key);
                match state.sync() {
                    Ok(()) => (),
                    Err(e) => {
                        error!("failed to sync: {}", e);
                    }
                }
            };

            increment_stat_counter("requests received");

            let event = match msg {
                GithubRequest::Fetch(query) => {
                    match query.search_for {
                        SearchFor::Repository => {
                            increment_stat_counter("repository fetch requests received");
                            let repos = fetch_all_repos(&github_v4, query, shutdown_handle.clone())?;
                            for repo in repos {
                                increment_stat_counter("repositories fetched");
                                callback(GithubEvent::RepositoryFetched(repo))
                            }
                            increment_stat_counter("repository fetch requests handled");
                        },
                        SearchFor::Undefined => panic!("search_for is Undefined: can't fetch an undefined entity")
                    }
                },
                GithubRequest::Fork(repo) => {
                    let fork = fork_repo(&github_v3, &repo)?;
                    callback(GithubEvent::RepositoryForked(fork))
                }
            };

            increment_stat_counter("requests handled");
            Ok(event)
        }
    };

    HandlingConsumer::builder()
        .pool_size(4)
        .subscribe(topic::GITHUB_REQUEST)
        .respond_to(topic::GITHUB_EVENT)
        .group(group::GITHUB)
        .handler(handler)
        .build()
        .expect("failed to build handler")
        .start(shutdown_handle)
        .expect("github service failed");
}

fn fetch_all_repos(gh: &GithubV4, query: IncompleteQuery, shutdown: GracefulShutdownHandle) -> Result<Vec<Repository>, HandlerError> {
    let mut repos = Vec::new();

    let mut page = None;
    let mut out_of_pages = false;

    while !out_of_pages && !shutdown.should_shutdown() {
        let query = query.clone();

        let query = if let Some(page) = page.take() {
            query.after(page).build()
        } else {
            query.build()
        };

        let query = query.map_err(|error| HandlerError::Internal { error })?;

        let data = search(&gh, query)
            .map_err(|error| HandlerError::Internal { error })?;

        let page_info = data.page_info;
        let nodes = data.nodes;
        repos.extend(nodes.into_iter());

        page = page_info.end_cursor;
        if !page_info.has_next_page {
            debug!("reached EOF");
            out_of_pages = true;
        };
    }

    Ok(repos)
}

use rustyrobot::github::v3::ExecutorExt;
use json::Value;

fn fork_repo(gh: &GithubV3, repo: &Repository) -> Result<Repository, HandlerError> {
    let endpoint = format!("repos/{}/forks", &repo.name_with_owner);
    debug!("fork endpoint: {}", endpoint);
    let value: Value = gh.post(()).custom_endpoint(&endpoint).send()
        .map_err(|error| HandlerError::Other { error })?;

    let fork = derive_fork(repo, value)
        .map_err(|error| HandlerError::Internal { error })?;

    Ok(fork)
}