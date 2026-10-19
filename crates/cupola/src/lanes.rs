//! Blocking jobs on lanes named by keys. A job is queued on the lane of each
//! of its keys when it is submitted, all of them at once, and runs on a
//! blocking thread once it heads every one of them; it holds them until it
//! has finished. One lane's jobs run one at a time in the order they were
//! queued, and jobs that share no key run concurrently.
//!
//! A [`Server`](crate::Server) handler that queues its request's job when it
//! is called runs one key's requests in the order they were sent, off the
//! connection's receive path.

use std::{
    collections::{HashMap, VecDeque},
    hash::Hash,
    panic::AssertUnwindSafe,
    sync::{Arc, Mutex, MutexGuard},
};

use futures::{FutureExt as _, channel::oneshot};

type Job = Box<dyn FnOnce() + Send>;

/// Clones are the same lanes.
pub struct Lanes<K> {
    inner: Arc<Mutex<State<K>>>,
}

struct State<K> {
    /// Each key's queue of jobs, by their ids. A key with none has no entry.
    lanes: HashMap<K, VecDeque<u64>>,
    /// Every queued job's keys, and the job until it is started.
    jobs: HashMap<u64, (Vec<K>, Option<Job>)>,
    next: u64,
}

impl<K> Clone for Lanes<K> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<K> Default for Lanes<K> {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(State {
                lanes: HashMap::new(),
                jobs: HashMap::new(),
                next: 0,
            })),
        }
    }
}

impl<K: Eq + Hash + Clone + Send + 'static> Lanes<K> {
    /// Queues `job` on `key`'s lane now; see [`Lanes::run_on`].
    pub fn run<T, F>(
        &self,
        key: K,
        job: F,
    ) -> impl Future<Output = anyhow::Result<T>> + Send + use<K, T, F>
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        self.run_on([key], job)
    }

    /// Queues `job` on the lane of each of `keys` now. The future resolves
    /// with what the job returned once it has run, or fails when the job
    /// panicked; it need not be polled for the job to run.
    pub fn run_on<I, T, F>(
        &self,
        keys: I,
        job: F,
    ) -> impl Future<Output = anyhow::Result<T>> + Send + use<I, K, T, F>
    where
        I: IntoIterator<Item = K>,
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        let (answer, answered) = oneshot::channel();
        let job: Job = Box::new(move || {
            let _ = answer.send(job());
        });
        let mut unique = Vec::new();
        for key in keys {
            if !unique.contains(&key) {
                unique.push(key);
            }
        }
        let started = {
            let mut state = self.lock();
            state.next += 1;
            let id = state.next;
            for key in &unique {
                state.lanes.entry(key.clone()).or_default().push_back(id);
            }
            state.jobs.insert(id, (unique, Some(job)));
            state.start(id)
        };
        if let Some((id, job)) = started {
            self.spawn(id, job);
        }
        answered.map(|answer| answer.map_err(|_| anyhow::anyhow!("the job panicked")))
    }

    fn spawn(&self, id: u64, job: Job) {
        let this = self.clone();
        blocking::unblock(move || {
            let _ = std::panic::catch_unwind(AssertUnwindSafe(job));
            this.finish(id);
        })
        .detach();
    }

    /// Takes job `id` off its lanes, and starts the jobs that now head all
    /// of theirs.
    fn finish(&self, id: u64) {
        let started: Vec<_> = {
            let mut state = self.lock();
            let Some((keys, _)) = state.jobs.remove(&id) else {
                return;
            };
            let mut next = Vec::new();
            for key in keys {
                let Some(lane) = state.lanes.get_mut(&key) else {
                    continue;
                };
                lane.pop_front();
                match lane.front() {
                    Some(head) => next.push(*head),
                    None => drop(state.lanes.remove(&key)),
                }
            }
            next.into_iter()
                .filter_map(|head| state.start(head))
                .collect()
        };
        for (id, job) in started {
            self.spawn(id, job);
        }
    }

    fn lock(&self) -> MutexGuard<'_, State<K>> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl<K: Eq + Hash> State<K> {
    /// Job `id`, taken to be run, when it heads every lane it is on and has
    /// not been started.
    fn start(&mut self, id: u64) -> Option<(u64, Job)> {
        let (keys, job) = self.jobs.get_mut(&id)?;
        let heads = keys
            .iter()
            .all(|key| self.lanes.get(key).and_then(VecDeque::front) == Some(&id));
        match heads {
            true => job.take().map(|job| (id, job)),
            false => None,
        }
    }
}
