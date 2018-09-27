extern crate rustyrobot_common as rustyrobot;
extern crate rdkafka;
extern crate ctrlc;
extern crate failure;
#[macro_use]
extern crate log;
#[macro_use]
extern crate fern;
extern crate chrono;

use std::sync::{Arc, Mutex};
use failure::Error;

use rustyrobot::{
    kafka::{
        topic, group,
        github::{GithubRequest, GithubEvent},
        util::{
            handler::HandlerThreadPool,
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
        .level_for("fetcher", log::LevelFilter::Debug)
        .level_for("rustyrobot_common", log::LevelFilter::Debug)
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

    let producer: ThreadedProducer<DefaultProducerContext> = ClientConfig::new()
        .set("bootstrap.servers", "127.0.0.1:9092")
        .set("produce.offset.report", "true")
        .set("message.timeout.ms", "5000")
        .create()
        .expect("failed to create producer");


    let handler = {
        let shutdown_handle = shutdown_handle.clone();
        move |msg, callback: &mut dyn FnMut(GithubEvent)| {
            let github_v3 = GithubV3::new(&token).expect("failed to create GitHub V3 API instance");
            let github_v4 = GithubV4::new(&token).expect("failed to create GitHub V4 API instance");

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
                GithubRequest::Fetch { date, query } => {
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
                }
            };

            increment_stat_counter("requests handled");
            Ok(event)
        }
    };

    HandlerThreadPool::builder()
        .pool_size(4)
        .subscribe(topic::GITHUB_REQUEST)
        .group(group::GITHUB)
        .handler(handler)
        .build()
        .expect("failed to build handler")
        .start(shutdown_handle)
        .expect("github service failed");
}

fn fetch_all_repos(gh: &GithubV4, query: IncompleteQuery, shutdown: GracefulShutdownHandle) -> Result<Vec<Repository>, Error> {
    let mut repos = Vec::new();

    let mut page = None;
    let mut out_of_pages = false;

    while !out_of_pages && !shutdown.should_shutdown() {
        let query = query.clone();

        let query = if let Some(page) = page.take() {
            query.after(page).build()?
        } else {
            query.build()?
        };

        let data = search(&gh, query)?;

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
