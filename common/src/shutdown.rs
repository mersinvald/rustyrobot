use std::sync::RwLock;
use std::sync::Arc;
use std::collections::HashSet;

#[derive(Clone)]
pub struct GracefulShutdown {
    threads: Arc<RwLock<HashSet<String>>>,
    shutdown_flag: Arc<RwLock<bool>>,
}

impl GracefulShutdown {
    pub fn new() -> Self {
        GracefulShutdown {
            threads: Arc::new(RwLock::new(HashSet::new())),
            shutdown_flag: Arc::new(RwLock::new(false)),
        }
    }

    pub fn thread_handle(&self) -> GracefulShutdownHandle {
        GracefulShutdownHandle::from(self.clone())
    }

    pub fn threads_running(&self) -> u64 {
        self.threads.read().unwrap().len() as u64
    }

    pub fn get_running_threads(&self) -> Vec<String> {
        self.threads.read().unwrap().iter().cloned().collect()
    }

    pub fn shutdown(&self) {
        *self.shutdown_flag.write().unwrap() = true
    }

    pub fn is_shutdown(&self) -> bool {
        *self.shutdown_flag.read().unwrap()
    }
}

#[derive(Clone)]
pub struct GracefulShutdownHandle {
    threads: Arc<RwLock<HashSet<String>>>,
    shutdown_flag: Arc<RwLock<bool>>,
}

impl From<GracefulShutdown> for GracefulShutdownHandle {
    fn from(gs: GracefulShutdown) -> Self {
        GracefulShutdownHandle {
            threads: gs.threads,
            shutdown_flag: gs.shutdown_flag,
        }
    }
}

impl GracefulShutdownHandle {
    /// # Panics
    /// Name collisions are not allowed, thread will panic
    pub fn started<T: Into<String>>(&self, name: T) -> GracefulShutdownStartedLock {
        let name = name.into();
        info!("started thread {:?}", name);
        let mut threads = self.threads.write().unwrap();
        if threads.contains(&name) {
            panic!("thread name collision on {:?}", name);
        } else {
            threads.insert(name.clone());
        }
        GracefulShutdownStartedLock {
            name,
            handle: self.clone()
        }
    }

    pub fn should_shutdown(&self) -> bool {
        *self.shutdown_flag.read().unwrap()
    }
}

pub struct GracefulShutdownStartedLock {
    name: String,
    handle: GracefulShutdownHandle,
}

impl Drop for GracefulShutdownStartedLock {
    fn drop(&mut self) {
        info!("stopping thread {:?}", self.name);
        self.handle.threads.write().unwrap().remove(&self.name);
    }
}
