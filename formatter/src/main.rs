extern crate rustyrobot;
extern crate rdkafka;
extern crate ctrlc;
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
extern crate git2;
extern crate tempdir;

mod git;

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
    github::utils::load_token,
    types::{Repository},
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
        .level_for("github", log::LevelFilter::Trace)
        .level_for("formatter", log::LevelFilter::Trace)
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

    HandlingConsumer::builder()
        .subscribe(topic::EVENT)
        .respond_to(topic::EVENT)
        .group(group::FORMATTER)
        .handler(|event, callback: &mut dyn FnMut(Event)| {
            match event {
                Event::RepositoryForked(repo) => {
                    //callback(Event::RepositoryFormatted(rustfmt_repo(repo)?))
                    rustfmt_repo(repo)?;
                },
                _ => ()
            }
            Ok(())
        })
        .build()
        .expect("failed to build handler")
        .start(shutdown_handle)
        .expect("formatter service failed");
}

use std::path::{Path, PathBuf};
use failure::err_msg;
use git::{Git, CheckoutMode, DirHistory};
use std::process::{Command, ExitStatus};

const RUSTFMT_BRANCH: &str = "rustyrobot_suggested_formatting";

fn rustfmt_repo(repo: Repository) -> Result<Repository, HandlerError> {
    let tempdir = tempdir::TempDir::new(&repo.name_with_owner.replace('/', "_")).map_err(HandlerError::internal)?;
    let path = tempdir.path();

    // Clone repo
    let mut git = Git::clone(&path, &repo.ssh_url)
        .map_err(HandlerError::internal)?;

    // Checkout default branch
    git.checkout(CheckoutMode::Branch { name: &repo.default_branch, create: false })
        .map_err(HandlerError::internal)?;

    // Sync with upstream
    // Add remote
    if !git.has_remote("upstream").map_err(HandlerError::internal)? {
        git.add_remote("upstream", &repo.parent.as_ref().unwrap().ssh_url)
            .map_err(HandlerError::internal)?;
    }
    git.fetch("upstream")
        .map_err(HandlerError::internal)?;
    git.merge(&format!("upstream/{}", repo.default_branch))
        .map_err(HandlerError::internal)?;
    git.push("master")
        .map_err(HandlerError::internal)?;

    // Checkout working branch
    if git.has_branch(RUSTFMT_BRANCH).map_err(HandlerError::internal)? {
        git.checkout(CheckoutMode::Branch { name: RUSTFMT_BRANCH, create: false })
            .map_err(HandlerError::internal)?;
        git.reset("HEAD~1", true)
            .map_err(HandlerError::internal)?;
        git.merge(&repo.default_branch)
            .map_err(HandlerError::internal)?;
    } else {
        git.checkout(CheckoutMode::Branch { name: RUSTFMT_BRANCH, create: true })
            .map_err(HandlerError::internal)?;
    }

    // Run code formatting
    format_code(&path)?;

    // Collect info about formatting results
    let repo = collect_metrics(&path, repo)?;

    // Commit and push changes
    git.commit_all("rustyrobot formatting")
        .map_err(HandlerError::internal)?;
    git.push(RUSTFMT_BRANCH)
        .map_err(HandlerError::internal)?;

    Ok(repo)
}

fn format_code(path: &Path) -> Result<(), HandlerError> {
    let mut history = DirHistory::new();
    let dirlock = history.pushd(path)
        .map_err(HandlerError::internal)?;

    let cmd = "cargo";
    let args = &[
        "fmt",
    ];

    let status = Command::new(cmd)
        .args(args)
        .status()
        .map_err(HandlerError::internal)?;

    if !status.success() {
        Err(HandlerError::internal(err_msg("failed to format repo")))
    } else {
        Ok(())
    }
}

fn collect_metrics(path: &Path, repo: Repository) -> Result<Repository, HandlerError> {
    Ok(repo)
}
