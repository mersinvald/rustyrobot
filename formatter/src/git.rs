// DISCLAIMER:
// I know doing as I did here is not true engineering approach.
// I know libgit2 exists.
// Also I know that libgit2 is a huge fucking mess that's impossible to use.
// I tried. It's insanely inhuman and designed for some Ubersoldaten.
// Regards to Linus Torvalds. His wicked CLI tool is actually more usable then it's C API.

use failure::Error;
use log;
use std::env;
use std::io::{BufRead, Cursor};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::str::Chars;

pub struct DirHistory {
    history: Vec<PathBuf>,
}

impl DirHistory {
    pub fn new() -> Self {
        DirHistory { history: vec![] }
    }

    pub fn pushd(&mut self, path: impl AsRef<Path>) -> Result<DirHistoryLock, Error> {
        let current = env::current_dir()?;
        self.history.push(current);
        env::set_current_dir(&path)?;
        Ok(DirHistoryLock(self))
    }

    pub fn popd(&mut self) -> Result<(), Error> {
        let path = self.history.pop().expect("directory buffer underflow");
        env::set_current_dir(&path)?;
        Ok(())
    }
}

pub struct DirHistoryLock<'a>(&'a mut DirHistory);

impl<'a> Drop for DirHistoryLock<'a> {
    fn drop(&mut self) {
        if let Err(e) = self.0.popd() {
            error!("failed to popd");
        }
    }
}

pub struct Git {
    history: DirHistory,
    repo_path: PathBuf,
}

impl Git {
    fn exec_git_cmd(args: &[&str]) -> Result<(), Error> {
        let mut cmd = Command::new("git");
        cmd.args(args);
        if !log_enabled!(log::Level::Trace) {
            cmd.stdout(Stdio::null());
            cmd.stderr(Stdio::null());
        }
        let status = cmd.status()?;
        if !status.success() {
            Err(GitError::CommandFailed {
                command: format!("git {}", args.join(" ")),
                status,
            }
            .into())
        } else {
            Ok(())
        }
    }

    fn exec_git_cmt_get_stdout(args: &[&str]) -> Result<Vec<u8>, Error> {
        let mut cmd = Command::new("git");
        cmd.args(args);
        let output = cmd.output()?;
        if !output.status.success() {
            Err(GitError::CommandFailed {
                command: format!("git {}", args.join(" ")),
                status: output.status,
            }
            .into())
        } else {
            Ok(output.stdout)
        }
    }

    pub fn clone(path: impl AsRef<Path>, repo_clone_url: &str) -> Result<Self, Error> {
        let path = path.as_ref().to_owned();
        let path_str = path
            .clone()
            .to_str()
            .map(ToOwned::to_owned)
            .ok_or(GitError::PathIsNotUtf8)?;

        Self::exec_git_cmd(&["clone", repo_clone_url, &path_str])?;

        Ok(Git {
            history: DirHistory::new(),
            repo_path: path,
        })
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref().to_owned();
        let path_str = path
            .clone()
            .to_str()
            .map(ToOwned::to_owned)
            .ok_or(GitError::PathIsNotUtf8)?;
        let mut history = DirHistory::new();

        {
            let dirlock = history.pushd(&path);
            Self::exec_git_cmd(&["status"])?;
        }

        Ok(Git {
            history,
            repo_path: path,
        })
    }

    pub fn remotes(&mut self) -> Result<Vec<Remote>, Error> {
        let dirlock = self.history.pushd(&self.repo_path)?;

        let stdout = Self::exec_git_cmt_get_stdout(&["remote", "-v"])?;

        let mut remotes = Vec::new();
        let reader = Cursor::new(stdout);
        for line in reader.lines() {
            let line = line?.replace("\t", " ");
            let mut tokens = line.split(" ");
            let name = tokens.next().ok_or(GitError::OutputTooShort)?;
            let url = tokens.next().ok_or(GitError::OutputTooShort)?;
            remotes.push(Remote {
                name: name.to_string(),
                url: url.to_string(),
            })
        }

        Ok(remotes)
    }

    pub fn has_remote(&mut self, name: &str) -> Result<bool, Error> {
        Ok(self.remotes()?.iter().any(|remote| remote.name == name))
    }

    pub fn add_remote(&mut self, name: &str, url: &str) -> Result<(), Error> {
        if self.has_remote(name)? {
            return Ok(());
        }

        let dirlock = self.history.pushd(&self.repo_path)?;
        Self::exec_git_cmd(&["remote", "add", name, url])
    }

    pub fn checkout(&mut self, checkout_mode: CheckoutMode) -> Result<(), Error> {
        let dirlock = self.history.pushd(&self.repo_path)?;

        let mut args = vec!["checkout"];
        match checkout_mode {
            CheckoutMode::Commit(name) => args.push(name),
            CheckoutMode::Branch { name, create } => {
                if create {
                    args.push("-b");
                }
                args.push(name)
            }
        }

        Self::exec_git_cmd(&args)
    }

    pub fn reset(&mut self, target: &str, hard: bool) -> Result<(), Error> {
        let dirlock = self.history.pushd(&self.repo_path)?;
        let cmd = "git";

        let mut args = vec!["reset"];
        if hard {
            args.push("--hard");
        }
        args.push(target);

        Self::exec_git_cmd(&args)
    }

    pub fn branches(&mut self) -> Result<Vec<Branch>, Error> {
        let dirlock = self.history.pushd(&self.repo_path)?;

        let stdout = Self::exec_git_cmt_get_stdout(&["branch", "-a"])?;

        let mut branches = Vec::new();
        let reader = Cursor::new(stdout);
        for line in reader.lines() {
            let line = line?.replace("\t", "").replace("*", "").replace(" ", "");
            if !line.contains("/") {
                branches.push(Branch {
                    location: BranchLocation::Local,
                    name: line,
                })
            } else {
                let mut tokens = line.split("/");
                let _ = tokens.next().ok_or(GitError::OutputTooShort)?;
                let remote = tokens.next().ok_or(GitError::OutputTooShort)?;
                let name = tokens.next().ok_or(GitError::OutputTooShort)?;
                branches.push(Branch {
                    location: BranchLocation::Remote(remote.to_string()),
                    name: name.to_string(),
                })
            }
        }

        Ok(branches)
    }

    pub fn has_branch(&mut self, name: &str) -> Result<bool, Error> {
        Ok(self.branches()?.iter().any(|b| b.name == name))
    }

    pub fn fetch(&mut self, target: &str) -> Result<(), Error> {
        let dirlock = self.history.pushd(&self.repo_path)?;

        Self::exec_git_cmd(&["fetch", target])
    }

    pub fn merge(&mut self, target: &str) -> Result<(), Error> {
        let dirlock = self.history.pushd(&self.repo_path)?;

        Self::exec_git_cmd(&["merge", target, "--no-edit"])
    }

    pub fn commit_all(&mut self, msg: &str) -> Result<(), Error> {
        let dirlock = self.history.pushd(&self.repo_path)?;

        Self::exec_git_cmd(&["commit", "-a", "-m", msg])
    }

    pub fn push(&mut self, target: &str) -> Result<(), Error> {
        let dirlock = self.history.pushd(&self.repo_path)?;

        Self::exec_git_cmd(&["push", "--set-upstream", "origin", target, "--force"])
    }

    pub fn diff_stat(&mut self, target: &str) -> Result<DiffStat, Error> {
        let dirlock = self.history.pushd(&self.repo_path)?;

        let stdout = Self::exec_git_cmt_get_stdout(&["diff", "--shortstat", target])?;

        let mut reader = Cursor::new(stdout);
        let mut stat_line = String::new();
        if reader.read_line(&mut stat_line)? == 0 {
            return Err(GitError::OutputTooShort.into());
        }

        Self::parse_shortstat_msg(&stat_line)
    }

    fn parse_shortstat_msg(msg: &str) -> Result<DiffStat, Error> {
        let read_number = |chars: &mut Chars| -> Result<u64, Error> {
            let num_str = chars
                .skip_while(|c| !c.is_digit(10))
                .take_while(|c| c.is_digit(10))
                .collect::<String>();
            if num_str.is_empty() {
                Err(GitError::OutputTooShort.into())
            } else {
                Ok(num_str.parse::<u64>()?)
            }
        };

        let mut chars = msg.chars();
        let files_changed = read_number(&mut chars)?;
        let lines_added = read_number(&mut chars)?;
        let lines_removed = read_number(&mut chars)?;

        Ok(DiffStat {
            files_changed,
            lines_added,
            lines_removed,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffStat {
    pub files_changed: u64,
    pub lines_added: u64,
    pub lines_removed: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    location: BranchLocation,
    name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BranchLocation {
    Local,
    Remote(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckoutMode<'a> {
    Commit(&'a str),
    Branch { name: &'a str, create: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remote {
    pub name: String,
    pub url: String,
}

#[derive(Clone, Debug, Fail)]
enum GitError {
    #[fail(display = "executing {:?} failed with status {}", command, status)]
    CommandFailed { command: String, status: ExitStatus },
    #[fail(display = "path is not a utf8 string")]
    PathIsNotUtf8,
    #[fail(display = "output was too shirt to parse it")]
    OutputTooShort,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::remove_dir_all;

    #[test]
    fn dirhistory_new() {
        let history = DirHistory::new();
    }

    #[test]
    fn dirhistory_pushd() {
        let mut history = DirHistory::new();
        let root = history.pushd("/").unwrap();
        assert_eq!(env::current_dir().unwrap(), PathBuf::from("/"));
    }

    #[test]
    fn dirhistory_lock_drop() {
        let mut history = DirHistory::new();
        let start = env::current_dir().unwrap();
        let root = history.pushd("/").unwrap();
        assert_eq!(env::current_dir().unwrap(), PathBuf::from("/"));
        drop(root);
        assert_eq!(env::current_dir().unwrap(), start);
    }

    #[test]
    #[should_panic]
    fn dirhistory_underflow() {
        let mut history = DirHistory::new();
        history.popd().unwrap();
    }

    #[test]
    fn git_clone() {
        let path = PathBuf::from("/tmp/test_git_clone");
        if path.exists() {
            remove_dir_all(&path).unwrap();
        }

        Git::clone(&path, "git@github.com:mersinvald/github-rustfmt-bot.git").unwrap();
        assert!(path.exists());

        remove_dir_all(&path).unwrap();
    }

    #[test]
    fn git_remotes() {
        let mut git = Git::open("./").unwrap();
        assert_eq!(
            git.remotes().unwrap(),
            vec![
                Remote {
                    name: "origin".to_string(),
                    url: "git@github.com:mersinvald/github-rustfmt-bot.git".to_string()
                },
                Remote {
                    name: "origin".to_string(),
                    url: "git@github.com:mersinvald/github-rustfmt-bot.git".to_string()
                },
            ]
        );
    }

    #[test]
    fn git_has_remote() {
        let mut git = Git::open("./").unwrap();
        assert_eq!(git.has_remote("origin").unwrap(), true);
        assert_eq!(git.has_remote("nonexistent").unwrap(), false);
    }

    #[test]
    fn git_add_remote() {
        let path = PathBuf::from("/tmp/test_git_add_remote");
        if path.exists() {
            remove_dir_all(&path).unwrap();
        }

        let mut git =
            Git::clone(&path, "git@github.com:mersinvald/github-rustfmt-bot.git").unwrap();
        assert!(path.exists());
        git.add_remote("new_remote", "https://localhost/").unwrap();
        assert_eq!(git.has_remote("new_remote").unwrap(), true);

        remove_dir_all(&path).unwrap();
    }

    #[test]
    fn git_checkout() {
        let path = PathBuf::from("/tmp/test_git_checkout");
        if path.exists() {
            remove_dir_all(&path).unwrap();
        }

        let mut git =
            Git::clone(&path, "git@github.com:mersinvald/github-rustfmt-bot.git").unwrap();
        assert!(path.exists());
        git.checkout(CheckoutMode::Branch {
            name: "new",
            create: true,
        })
        .unwrap();

        remove_dir_all(&path).unwrap();
    }

    #[test]
    fn git_branches() {
        let mut git = Git::open("./").unwrap();
        assert_eq!(
            git.branches().unwrap(),
            vec![
                Branch {
                    name: "master".to_string(),
                    location: BranchLocation::Local
                },
                Branch {
                    name: "master".to_string(),
                    location: BranchLocation::Remote("origin".to_string())
                },
            ]
        );
    }

    #[test]
    fn git_parse_shortstat() {
        assert_eq!(
            Git::parse_shortstat_msg(" 1 file changed, 2 insertions(+), 1 deletion(-)").unwrap(),
            DiffStat {
                files_changed: 1,
                lines_added: 2,
                lines_removed: 1
            }
        );
        assert_eq!(
            Git::parse_shortstat_msg(" 100 file changed, 2123 insertions(+), 19999999 deletion(-)")
                .unwrap(),
            DiffStat {
                files_changed: 100,
                lines_added: 2123,
                lines_removed: 19999999
            }
        );
    }
}
