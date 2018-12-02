extern crate rustyrobot;
extern crate rdkafka;
extern crate ctrlc;
extern crate failure;
#[macro_use]
extern crate log;
extern crate fern;
extern crate chrono;
extern crate github_rs as github_v3;
extern crate serde_derive;
extern crate serde;
extern crate serde_json as json;

use std::sync::{Arc, Mutex};
use failure::Error;

use rustyrobot::{
    kafka::{
        topic, group,
        Event,
        GithubRequest,
        util::{
            handler::{HandlingConsumer, HandlerError},
            state::StateHandler,
        }
    },
    github::v4::Github as GithubV4,
    github::v3::Github as GithubV3,
    github::utils::{load_token, load_username},
    types::{Repository, Notification, PR, PRStatus},
    search::{
        search,
        query::SearchFor,
        query::IncompleteQuery,
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
    let username = load_username()
        .expect("failed to load username (set GITHUB_USERNAME env)");

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
                GithubRequest::Fetch(query) => {
                    match query.search_for {
                        SearchFor::Repository => {
                            increment_stat_counter("repository fetch requests received");
                            let repos = fetch_all_repos(&github_v4, query, shutdown_handle.clone())?;
                            for repo in repos {
                                increment_stat_counter("repositories fetched");
                                callback(Event::RepositoryFetched(repo))
                            }
                            increment_stat_counter("repository fetch requests handled");
                        },
                        SearchFor::Undefined => panic!("search_for is Undefined: can't fetch an undefined entity")
                    }
                },
                GithubRequest::Fork(repo) => {
                    let fork = fork_repo(&github_v3, &repo)?;
                    callback(Event::RepositoryForked(fork))
                }
                GithubRequest::DeleteFork(repo) => {
                    delete_repo(&github_v3, &repo.name_with_owner)?;
                    callback(Event::ForkDeleted(repo))
                },
                GithubRequest::CreatePR {repo, branch, title, message} => {
                    let repo = create_pr(&github_v3, repo, &branch, &title, &message)?;
                    callback(Event::PRCreated(repo))
                },
                GithubRequest::FetchNotifications => {
                    // TODO
                    let events = fetch_notifications(&github_v3, &username)?;
                    for event in events {
                        callback(Event::Notification(event));
                    }
                },
                GithubRequest::CheckPRStatus(repo) => {
                    let repo_none_if_unchanged = fetch_pr_status(&github_v3, repo)?;
                    if let Some(repo) = repo_none_if_unchanged {
                        callback(Event::PRStatusChange(repo))
                    }
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
use rustyrobot::github::v3::{EmptyResponse, ExecutorExt};
use rustyrobot::search::NodeType;
use json::Value;
use failure::err_msg;
use std::collections::HashMap;

fn fork_repo(gh: &GithubV3, parent: &Repository) -> Result<Repository, HandlerError> {
    let endpoint = format!("repos/{}/forks", &parent.name_with_owner);
    debug!("fork endpoint: {}", endpoint);
    let value: Value = gh.post(())
        .custom_endpoint(&endpoint)
        .send(&[StatusCode::Accepted])
        .map_err(|error| HandlerError::Other { error })?;

    let fork = Repository::from_value(value)
        .map_err(|error| HandlerError::Internal { error })?;

    Ok(fork)
}

fn delete_repo(gh: &GithubV3, repo_name: &str) -> Result<(), HandlerError> {
    let endpoint = format!("repos/{}", repo_name);

    let _value: EmptyResponse = gh.delete(())
        .custom_endpoint(&endpoint)
        .send(&[StatusCode::NoContent])
        .map_err(|error| HandlerError::Internal { error })?;

    Ok(())
}

fn create_pr(gh: &GithubV3, mut repo: Repository, branch: &str, title: &str, message: &str) -> Result<Repository, HandlerError> {
    let parent = repo.parent.clone().ok_or(HandlerError::Internal {
        error: err_msg("parent is empty, can't issue a PR")
    })?;

    let owner = repo.name_with_owner.split("/").next().unwrap().to_string();
    let head = format!("{}:{}", owner, branch);

    if pr_exists(gh, &parent.name_with_owner, &head)? {
        warn!("pull request {} -> {} exists, refusing to create", head, parent.name_with_owner);
        return Ok(repo)
    }

    let response: Value = {
        let endpoint = format!("repos/{}/pulls", parent.name_with_owner);
        let mut body: HashMap<&str, &str> = HashMap::new();
        body.insert("title", title);
        body.insert("body", message);
        body.insert("base", &repo.default_branch);
        body.insert("head", &head);

        gh.post(body)
            .custom_endpoint(&endpoint)
            .send(&[StatusCode::Created])
            .map_err(|error| HandlerError::Internal { error })?
    };

    let pr_number = response.as_object()
        .and_then(|obj| obj.get("number"))
        .and_then(|number| number.as_i64())
        .ok_or_else(|| HandlerError::Internal {
            error: err_msg("number is missing for Pull Request")
        })?;

    let mut stats = repo.stats.take().unwrap_or_default();
    let pr = PR {
        title: title.to_owned(),
        number: pr_number,
        status: PRStatus::Open,
    };

    // Remove previous entry if exists
    if let Some(pos) = stats.prs.iter().position(|pr| pr.number == pr_number) {
        stats.prs.remove(pos);
    }

    stats.prs.push(pr);
    repo.stats = Some(stats);

    Ok(repo)
}

fn pr_exists(gh: &GithubV3, name_with_owner: &str, head: &str) -> Result<bool, HandlerError> {
    let endpoint = format!("repos/{}/pulls?head={}", name_with_owner, head);
    let response: Value = gh.get()
        .custom_endpoint(&endpoint)
        .send(&[StatusCode::Ok])
        .map_err(|error| HandlerError::Internal { error })?;
    Ok(!response.as_array().map(Vec::is_empty).unwrap_or(true))
}

fn fetch_notifications(gh: &GithubV3, username: &str) -> Result<Vec<Notification>, HandlerError> {
    let endpoint = "notifications";
    let response: Value = gh.get()
        .custom_endpoint(&endpoint)
        .send(&[StatusCode::Ok])
        .map_err(|error| HandlerError::Internal { error })?;
    println!("{:#?}", response);
    Ok(vec![])
}

fn fetch_pr_status(gh: &GithubV3, mut repo: Repository) -> Result<Option<Repository>, HandlerError> {
    let mut stats = repo.stats.take().unwrap_or_default();
    let old_prs = stats.prs.clone();

    let mut new_prs = Vec::with_capacity(old_prs.len());
    for pr in stats.prs {
        let endpoint = format!("repos/{}/pulls/{}", repo.name_with_owner, pr.number);
        let response: Value = gh.get()
            .custom_endpoint(&endpoint)
            .send(&[StatusCode::Ok])
            .map_err(|error| HandlerError::Internal { error })?;

        let pr_obj = response.as_object()
            .ok_or_else(|| HandlerError::internal(err_msg("GET PR returned <non object>")))?;

        let pr_title = pr_obj.get("title").and_then(|t| t.as_str())
            .ok_or_else(|| HandlerError::internal(err_msg("no title associated with PR")))?;

        let pr_number = pr_obj.get("number").and_then(|t| t.as_i64())
            .ok_or_else(|| HandlerError::internal(err_msg("no number associated with PR")))?;

        let pr_status = pr_obj.get("state")
            .and_then(|t| t.as_str())
            .and_then(|t| PRStatus::from_str(t))
            .ok_or_else(|| HandlerError::internal(err_msg("no state associated with PR")))?;

        new_prs.push(PR {
            title: pr_title.to_string(),
            number: pr_number,
            status: pr_status
        });
    }

    if new_prs != old_prs {
        stats.prs = new_prs;
        repo.stats = Some(stats);
        Ok(Some(repo))
    } else {
        Ok(None)
    }
}
