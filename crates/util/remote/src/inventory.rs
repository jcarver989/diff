use crate::{Instance, InstanceState, Lifecycle, RemoteError, RemoteSpec, Remotes};
use std::collections::HashMap;
use std::mem;
use std::time::Duration;

pub const POLL: Duration = Duration::from_secs(15);
pub const POLL_SETTLING: Duration = Duration::from_secs(3);

pub struct Inventory {
    remotes: Vec<RemoteSpec>,
    listed: HashMap<String, Result<Vec<Instance>, String>>,
    refreshing: bool,
    refresh_again: bool,
    changing: usize,
    changes: usize,
}

pub struct Listing {
    changes: usize,
    remotes: Vec<RemoteSpec>,
}

pub struct Listed {
    changes: usize,
    results: Vec<(String, Result<Vec<Instance>, RemoteError>)>,
}

impl Inventory {
    #[must_use]
    pub fn new(remotes: Vec<RemoteSpec>) -> Self {
        let mut inventory = Self {
            remotes: Vec::new(),
            listed: HashMap::new(),
            refreshing: false,
            refresh_again: false,
            changing: 0,
            changes: 0,
        };
        inventory.set_remotes(remotes);
        inventory
    }

    #[must_use]
    pub fn remotes(&self) -> &[RemoteSpec] {
        &self.remotes
    }

    #[must_use]
    pub fn remote(&self, name: &str) -> Option<&RemoteSpec> {
        self.remotes.iter().find(|remote| remote.name == name)
    }

    pub fn set_remotes(&mut self, remotes: Vec<RemoteSpec>) {
        self.listed.retain(|name, _| {
            let kept = |remote: &&RemoteSpec| remote.name == *name;
            remotes
                .iter()
                .find(kept)
                .zip(self.remotes.iter().find(kept))
                .is_some_and(|(new, old)| {
                    !new.is_static()
                        && new.list == old.list
                        && new.dir == old.dir
                        && new.vars == old.vars
                })
        });
        for remote in remotes.iter().filter(|remote| remote.is_static()) {
            let instance = Instance {
                id: remote.name.clone(),
                state: InstanceState::Running,
                expires_at: None,
            };
            self.listed.insert(remote.name.clone(), Ok(vec![instance]));
        }
        self.remotes = remotes;
    }

    #[must_use]
    pub fn has_listed(&self) -> bool {
        self.remotes
            .iter()
            .all(|remote| self.listed.contains_key(&remote.name))
    }

    #[must_use]
    pub fn instances(&self, remote: &str) -> &[Instance] {
        self.listing(remote)
            .and_then(Result::ok)
            .unwrap_or_default()
    }

    #[must_use]
    pub fn listing(&self, remote: &str) -> Option<Result<&[Instance], &str>> {
        self.listed
            .get(remote)
            .map(|listed| listed.as_deref().map_err(String::as_str))
    }

    #[must_use]
    pub fn instance(&self, remote: &str, id: &str) -> Option<&Instance> {
        self.instances(remote)
            .iter()
            .find(|instance| instance.id == id)
    }

    #[must_use]
    pub fn problem(&self, remote: &str) -> Option<&str> {
        self.listing(remote)?.err()
    }

    pub fn begin_listing(&mut self) -> Option<Listing> {
        if self.refreshing {
            self.refresh_again = true;
            return None;
        }
        if self.changing > 0 {
            return None;
        }
        let remotes: Vec<RemoteSpec> = self
            .remotes
            .iter()
            .filter(|remote| !remote.is_static())
            .cloned()
            .collect();
        if remotes.is_empty() {
            return None;
        }
        self.refreshing = true;
        Some(Listing {
            changes: self.changes,
            remotes,
        })
    }

    pub fn finish_listing(&mut self, listed: Listed) -> bool {
        self.refreshing = false;
        if listed.changes == self.changes {
            for (name, result) in listed.results {
                if self.remote(&name).is_some_and(|remote| !remote.is_static()) {
                    self.listed
                        .insert(name, result.map_err(|error| error.to_string()));
                }
            }
        } else {
            self.refresh_again = true;
        }
        mem::take(&mut self.refresh_again)
    }

    pub fn begin_change(&mut self, remote: &str, id: &str, change: Lifecycle) {
        if let Some(Ok(instances)) = self.listed.get_mut(remote) {
            for instance in instances.iter_mut().filter(|instance| instance.id == id) {
                instance.state = match change {
                    Lifecycle::Suspend => InstanceState::Suspending,
                    Lifecycle::Resume => InstanceState::Starting,
                    Lifecycle::Stop => InstanceState::Stopping,
                };
            }
        }
        self.changing += 1;
        self.changes += 1;
    }

    pub fn finish_change(&mut self) {
        self.changing = self.changing.saturating_sub(1);
        self.changes += 1;
    }

    pub fn launched(&mut self, remote: &str, id: &str) {
        let instance = Instance {
            id: id.to_owned(),
            state: InstanceState::Starting,
            expires_at: None,
        };
        match self.listed.get_mut(remote) {
            Some(Ok(instances)) if instances.iter().any(|listed| listed.id == id) => {}
            Some(Ok(instances)) => instances.push(instance),
            _ => drop(self.listed.insert(remote.to_owned(), Ok(vec![instance]))),
        }
        self.changes += 1;
    }

    #[must_use]
    pub fn is_settling(&self) -> bool {
        self.listed
            .values()
            .filter_map(|listed| listed.as_ref().ok())
            .flatten()
            .any(|instance| instance.state.is_settling())
    }

    #[must_use]
    pub fn poll_interval(&self) -> Duration {
        if self.is_settling() {
            POLL_SETTLING
        } else {
            POLL
        }
    }
}

impl Listing {
    pub fn run(self, remotes: &dyn Remotes) -> Listed {
        let results = self
            .remotes
            .iter()
            .map(|remote| (remote.name.clone(), remotes.list(remote)))
            .collect();
        Listed {
            changes: self.changes,
            results,
        }
    }
}
