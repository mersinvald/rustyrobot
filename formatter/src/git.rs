// DISCLAIMER:
// I know doing as I did here is not true engineering approach.
// I know libgit2 exists.
// Also I know that libgit2 is a huge fucking mess that's impossible to use.
// I tried. It's insanely inhuman and designed for some Ubersoldaten.
// Regards to Linus Torvalds. His wicked CLI tool is actually more usable then it's C API.


use std::path::{PathBuf, Path};
use failure::Error;
use std::env;
use std::process::{Command, ExitStatus, Stdio};
use std::io::{Cursor, BufRead};

pub struct DirHistory {
    history: Vec<PathBuf>,
}

impl DirHistory {
    pub fn new() -> Self {
        DirHistory {
            history: vec![],
        }
    }

    pub fn pushd(&mut self, path: impl AsRef<Path>) -> Result<DirHistoryLock, Error> {
        let current = env::current_dir()?;
        self.history.push(current);
        env::set_current_dir(&path)?;
        Ok(DirHistoryLock(self))
    }

    pub fn popd(&mut self) -> Result<(), Error> {
        let path = self.history.pop()
            .expect("directory buffer underflow");
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
    pub fn clone(path: impl AsRef<Path>, repo_clone_url: &str) -> Result<Self, Error> {
        let path = path.as_ref().to_owned();
        let path_str = path.clone().to_str().map(ToOwned::to_owned).ok_or(GitError::PathIsNotUtf8)?;

        let cmd = "git";
        let args = &[
            "clone",
            repo_clone_url,
            &path_str,
        ];

        let status = Command::new(cmd)
            .args(args)
            .status()?;

        if !status.success() {
            Err(GitError::CommandFailed {
                command: format!("{} {}", cmd, args.join(" ")),
                status
            }.into())
        } else {
            Ok(Git {
                history: DirHistory::new(),
                repo_path: path
            })
        }
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref().to_owned();
        let path_str = path.clone().to_str().map(ToOwned::to_owned).ok_or(GitError::PathIsNotUtf8)?;
        let mut history = DirHistory::new();

        let cmd = "git";
        let args = &[
            "status",
        ];

        let status = {
            let dirlock = history.pushd(&path);
            Command::new(cmd)
                .stdout(Stdio::null())
                .args(args)
                .status()?
        };

        if !status.success() {
            Err(GitError::CommandFailed {
                command: format!("{} {}", cmd, args.join(" ")),
                status
            }.into())
        } else {
            Ok(Git {
                history,
                repo_path: path
            })
        }
    }

    pub fn remotes(&mut self) -> Result<Vec<Remote>, Error> {
        let dirlock = self.history.pushd(&self.repo_path)?;
        let cmd = "git";
        let args = &[
            "remote",
            "-v",
        ];

        let output = Command::new(cmd)
            .args(args)
            .output()?;

        if !output.status.success() {
            return Err(GitError::CommandFailed {
                command: format!("{} {}", cmd, args.join(" ")),
                status: output.status
            }.into())
        }

        let mut remotes = Vec::new();
        let reader = Cursor::new(output.stdout);
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
        let cmd = "git";
        let args = &[
            "remote",
            "add",
            name,
            url
        ];

        let status = Command::new(cmd)
            .stdout(Stdio::null())
            .args(args)
            .status()?;

        if !status.success() {
            Err(GitError::CommandFailed {
                command: format!("{} {}", cmd, args.join(" ")),
                status: status
            }.into())
        } else {
            Ok(())
        }
    }

    pub fn checkout<'a>(&mut self, checkout_mode: CheckoutMode<'a>) -> Result<(), Error> {
        let dirlock = self.history.pushd(&self.repo_path)?;
        let cmd = "git";
        let mut args = vec!["checkout"];

        match checkout_mode {
            CheckoutMode::Commit(name) => {
                args.push(name)
            },
            CheckoutMode::Branch { name, create } => {
                if create {
                    args.push("-b");
                }
                args.push(name)
            }
        }

        let status = Command::new(cmd)
            .stdout(Stdio::null())
            .args(&args)
            .status()?;

        if !status.success() {
            Err(GitError::CommandFailed {
                command: format!("{} {}", cmd, args.join(" ")),
                status: status
            }.into())
        } else {
            Ok(())
        }
    }

    pub fn reset(&mut self, target: &str, hard: bool) -> Result<(), Error> {
        let dirlock = self.history.pushd(&self.repo_path)?;
        let cmd = "git";

        let mut args = vec!["reset"];
        if hard {
            args.push("--hard");
        }
        args.push(target);

        let status = Command::new(cmd)
            .stdout(Stdio::null())
            .args(&args)
            .status()?;

        if !status.success() {
            Err(GitError::CommandFailed {
                command: format!("{} {}", cmd, args.join(" ")),
                status: status
            }.into())
        } else {
            Ok(())
        }
    }

    pub fn branches(&mut self) -> Result<Vec<Branch>, Error> {
        let dirlock = self.history.pushd(&self.repo_path)?;
        let cmd = "git";
        let args = &[
            "branch",
            "-a",
        ];

        let output = Command::new(cmd)
            .args(args)
            .output()?;

        if !output.status.success() {
            return Err(GitError::CommandFailed {
                command: format!("{} {}", cmd, args.join(" ")),
                status: output.status
            }.into())
        }

        let mut branches = Vec::new();
        let reader = Cursor::new(output.stdout);
        for line in reader.lines() {
            let line = line?
                .replace("\t", "")
                .replace("*", "")
                .replace(" ", "");
            if !line.contains("/") {
                branches.push(Branch {
                    location: BranchLocation::Local,
                    name: line
                })
            } else {
                let mut tokens = line.split("/");
                let _ = tokens.next().ok_or(GitError::OutputTooShort)?;
                let remote = tokens.next().ok_or(GitError::OutputTooShort)?;
                let name = tokens.next().ok_or(GitError::OutputTooShort)?;
                branches.push(Branch {
                    location: BranchLocation::Remote(remote.to_string()),
                    name: name.to_string()
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
        let cmd = "git";
        let args = &[
            "fetch",
            target
        ];

        let status = Command::new(cmd)
            .stdout(Stdio::null())
            .args(args)
            .status()?;

        if !status.success() {
            Err(GitError::CommandFailed {
                command: format!("{} {}", cmd, args.join(" ")),
                status
            }.into())
        } else {
            Ok(())
        }
    }

    pub fn merge(&mut self, target: &str) -> Result<(), Error> {
        let dirlock = self.history.pushd(&self.repo_path)?;
        let cmd = "git";
        let args = &[
            "merge",
            target,
            "--no-edit",
        ];

        let status = Command::new(cmd)
            .stdout(Stdio::null())
            .args(args)
            .status()?;

        if !status.success() {
            Err(GitError::CommandFailed {
                command: format!("{} {}", cmd, args.join(" ")),
                status
            }.into())
        } else {
            Ok(())
        }
    }

    pub fn commit_all(&mut self, msg: &str) -> Result<(), Error> {
        let dirlock = self.history.pushd(&self.repo_path)?;
        let cmd = "git";
        let args = &[
            "commit",
            "-a",
            "-m",
            msg,
        ];

        let status = Command::new(cmd)
            .stdout(Stdio::null())
            .args(args)
            .status()?;

        if !status.success() {
            Err(GitError::CommandFailed {
                command: format!("{} {}", cmd, args.join(" ")),
                status
            }.into())
        } else {
            Ok(())
        }
    }

    pub fn push(&mut self, target: &str) -> Result<(), Error> {
        let dirlock = self.history.pushd(&self.repo_path)?;
        let cmd = "git";
        let args = &[
            "push",
            "--set-upstream",
            "origin",
            target
        ];

        let status = Command::new(cmd)
            .stdout(Stdio::null())
            .args(args)
            .status()?;

        if !status.success() {
            Err(GitError::CommandFailed {
                command: format!("{} {}", cmd, args.join(" ")),
                status
            }.into())
        } else {
            Ok(())
        }
    }
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
    Branch {
        name: &'a str,
        create: bool,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remote {
    pub name: String,
    pub url: String,
}

#[derive(Clone, Debug, Fail)]
enum GitError {
    #[fail(display = "executing {:?} failed with status {}", command, status)]
    CommandFailed {
        command: String,
        status: ExitStatus,
    },
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
        assert_eq!(git.remotes().unwrap(), vec![
            Remote { name: "origin".to_string(), url: "git@github.com:mersinvald/github-rustfmt-bot.git".to_string() },
            Remote { name: "origin".to_string(), url: "git@github.com:mersinvald/github-rustfmt-bot.git".to_string() },
        ]);
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

        let mut git = Git::clone(&path, "git@github.com:mersinvald/github-rustfmt-bot.git").unwrap();
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

        let mut git = Git::clone(&path, "git@github.com:mersinvald/github-rustfmt-bot.git").unwrap();
        assert!(path.exists());
        git.checkout(CheckoutMode::Branch { name: "new", create: true }).unwrap();

        remove_dir_all(&path).unwrap();
    }

    #[test]
    fn git_branches() {
        let mut git = Git::open("./").unwrap();
        assert_eq!(git.branches().unwrap(), vec![
            Branch { name: "master".to_string(), location: BranchLocation::Local },
            Branch { name: "master".to_string(), location: BranchLocation::Remote("origin".to_string()) },
        ]);
    }
}
