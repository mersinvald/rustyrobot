extern crate ctrlc;
extern crate failure;
extern crate rdkafka;
extern crate rustyrobot;
#[macro_use]
extern crate log;
extern crate chrono;
extern crate fern;
extern crate github_rs as github_v3;
extern crate serde;
extern crate serde_derive;
extern crate serde_json as json;

use failure::Error;
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
        .level_for("github", log::LevelFilter::Trace)
        .level_for("rustyrobot", log::LevelFilter::Trace)
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
    })
    .unwrap();

    let mut state = StateHandler::new(topic::GITHUB_STATE).expect("failed to open state");
    state.restore().expect("failed to restore state");
    let state = Arc::new(Mutex::new(state));

    let token = load_token().expect("failed to load token (set GITHUB_TOKEN env)");

    let github_v3 = GithubV3::new(&token).expect("failed to create GitHub V3 API instance");
    let github_v4 = GithubV4::new(&token).expect("failed to create GitHub V4 API instance");

    let handler = {
        let shutdown_handle = shutdown_handle.clone();
        move |msg, callback: &mut dyn FnMut(Event)| {
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

            match msg {
                GithubRequest::Fetch(query) => match query.search_for {
                    SearchFor::Repository => {
                        increment_stat_counter("repository fetch requests received");
                        let repos = fetch_all_repos(&github_v4, query, shutdown_handle.clone())?;
                        for repo in repos {
                            increment_stat_counter("repositories fetched");
                            callback(Event::RepositoryFetched(repo))
                        }
                        increment_stat_counter("repository fetch requests handled");
                    }
                    SearchFor::Undefined => {
                        panic!("search_for is Undefined: can't fetch an undefined entity")
                    }
                },
                GithubRequest::Fork(repo) => {
                    let fork = fork_repo(&github_v3, &repo)?;
                    callback(Event::RepositoryForked(fork))
                }
                GithubRequest::DeleteFork(repo) => {
                    delete_repo(&github_v3, &repo.name_with_owner)?;
                    callback(Event::ForkDeleted(repo))
                }
            };

            increment_stat_counter("requests handled");
            Ok(())
        }
    };

    HandlingConsumer::builder()
        .subscribe(topic::GITHUB_REQUEST)
        .respond_to(topic::EVENT)
        .group(group::GITHUB)
        .handler(handler)
        .build()
        .expect("failed to build handler")
        .start(shutdown_handle)
        .expect("github service failed");
}

use rustyrobot::types::repo;

fn fetch_all_repos(
    gh: &GithubV4,
    query: IncompleteQuery,
    shutdown: GracefulShutdownHandle,
) -> Result<Vec<Repository>, HandlerError> {
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

        let data = search::<repo::v4::Repository>(&gh, query)
            .map_err(|error| HandlerError::Internal { error })?;

        let page_info = data.page_info;
        let nodes = data.nodes;

        repos.extend(nodes.into_iter().map(Repository::from));

        page = page_info.end_cursor;
        if !page_info.has_next_page {
            debug!("reached EOF");
            out_of_pages = true;
        };
    }

    Ok(repos)
}

use github_v3::StatusCode;
use json::Value;
use rustyrobot::github::v3::{EmptyResponse, ExecutorExt};
use rustyrobot::search::NodeType;

fn fork_repo(gh: &GithubV3, parent: &Repository) -> Result<Repository, HandlerError> {
    let endpoint = format!("repos/{}/forks", &parent.name_with_owner);
    debug!("fork endpoint: {}", endpoint);
    let value: Value = gh
        .post(())
        .custom_endpoint(&endpoint)
        .send(&[StatusCode::Accepted])
        .map_err(|error| HandlerError::Other { error })?;

    let fork = Repository::from_value(value).map_err(|error| HandlerError::Internal { error })?;

    Ok(fork)
}

fn delete_repo(gh: &GithubV3, repo_name: &str) -> Result<(), HandlerError> {
    let endpoint = format!("repos/{}", repo_name);

    let _value: EmptyResponse = gh
        .delete(())
        .custom_endpoint(&endpoint)
        .send(&[StatusCode::NoContent])
        .map_err(|error| HandlerError::Internal { error })?;

    Ok(())
}
