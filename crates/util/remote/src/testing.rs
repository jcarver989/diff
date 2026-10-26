use crate::{Instance, InstanceState, Lifecycle, RemoteError, RemoteSpec, Remotes};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::SystemTime;

#[derive(Clone, Default)]
pub struct FakeRemotes {
    state: Arc<Mutex<State>>,
}

#[derive(Default)]
struct State {
    instances: HashMap<String, Vec<Instance>>,
    failures: HashMap<String, String>,
    launches: usize,
    listings: usize,
}

impl FakeRemotes {
    pub fn add(&self, remote: &str, id: &str, state: InstanceState, expires_at: Option<SystemTime>) {
        let instance = Instance { id: id.to_owned(), state, expires_at };
        self.lock().instances.entry(remote.to_owned()).or_default().push(instance);
    }

    #[must_use]
    pub fn instances(&self, remote: &str) -> Vec<Instance> {
        self.lock().instances.get(remote).cloned().unwrap_or_default()
    }

    #[must_use]
    pub fn state_of(&self, remote: &str, id: &str) -> Option<InstanceState> {
        self.instances(remote).into_iter().find(|instance| instance.id == id).map(|instance| instance.state)
    }

    pub fn fail(&self, remote: &str, message: &str) {
        self.lock().failures.insert(remote.to_owned(), message.to_owned());
    }

    #[must_use]
    pub fn launches(&self) -> usize {
        self.lock().launches
    }

    #[must_use]
    pub fn listings(&self) -> usize {
        self.lock().listings
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn check(&self, remote: &RemoteSpec) -> Result<MutexGuard<'_, State>, RemoteError> {
        let state = self.lock();
        match state.failures.get(&remote.name) {
            Some(message) => Err(RemoteError::Failed(message.clone())),
            None => Ok(state),
        }
    }
}

impl Remotes for FakeRemotes {
    fn list(&self, remote: &RemoteSpec) -> Result<Vec<Instance>, RemoteError> {
        let mut state = self.check(remote)?;
        state.listings += 1;
        let instances = state.instances.get(&remote.name).cloned().unwrap_or_default();
        Ok(instances.into_iter().filter(|instance| instance.state != InstanceState::Ended).collect())
    }

    fn launch(&self, remote: &RemoteSpec) -> Result<String, RemoteError> {
        let mut state = self.check(remote)?;
        let id = format!("instance-{}", state.launches);
        state.launches += 1;
        let instance = Instance { id: id.clone(), state: InstanceState::Running, expires_at: None };
        state.instances.entry(remote.name.clone()).or_default().push(instance);
        Ok(id)
    }

    fn change(&self, remote: &RemoteSpec, id: &str, change: Lifecycle) -> Result<(), RemoteError> {
        let mut state = self.check(remote)?;
        let instances = state.instances.entry(remote.name.clone()).or_default();
        let instance = instances
            .iter_mut()
            .find(|instance| instance.id == id)
            .ok_or_else(|| RemoteError::Failed(format!("{id} does not exist")))?;
        instance.state = match change {
            Lifecycle::Suspend => InstanceState::Suspended,
            Lifecycle::Resume => InstanceState::Running,
            Lifecycle::Stop => InstanceState::Ended,
        };
        Ok(())
    }
}
