//! The conversion queue. [`Runner`] runs jobs on worker threads with the same
//! limits as the CLI (a concurrency cap, one video at a time) and reports
//! [`Update`]s over a channel. [`Queue`] is the UI's view of every job.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use convt_core::{Cancel, Category, Error, Format, Job, Registry, format_by_extension};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};

use crate::history::Setup;

pub type JobId = u64;

/// A finished job's error, kept as text so it can be shown and stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobError {
    pub kind: &'static str,
    pub message: String,
}

impl From<&Error> for JobError {
    fn from(e: &Error) -> Self {
        Self {
            kind: e.kind(),
            message: e.to_string(),
        }
    }
}

pub type JobResult = Result<Vec<PathBuf>, JobError>;

#[derive(Debug, Clone, PartialEq)]
pub enum Update {
    Started(JobId),
    Progress(JobId, Option<f32>),
    Finished(JobId, JobResult),
}

fn is_video(job: &Job) -> bool {
    job.to.category == Category::Video
        || format_by_extension(&job.input).is_some_and(|f| f.category == Category::Video)
}

struct Pending {
    id: JobId,
    job: Job,
    video: bool,
}

#[derive(Default)]
struct State {
    queue: VecDeque<Pending>,
    running: HashMap<JobId, Cancel>,
    video_running: bool,
    concurrency: usize,
}

/// Runs jobs in the background. Cheap to clone.
#[derive(Clone)]
pub struct Runner {
    /// Replaced when the document pack is installed or removed; running jobs
    /// keep the registry they started with.
    registry: Arc<Mutex<Arc<Registry>>>,
    state: Arc<Mutex<State>>,
    tx: UnboundedSender<Update>,
}

impl Runner {
    pub fn new(registry: Arc<Registry>, concurrency: usize) -> (Self, UnboundedReceiver<Update>) {
        let (tx, rx) = unbounded();
        let state = State {
            concurrency: concurrency.max(1),
            ..State::default()
        };
        let runner = Self {
            registry: Arc::new(Mutex::new(registry)),
            state: Arc::new(Mutex::new(state)),
            tx,
        };
        (runner, rx)
    }

    pub fn submit(&self, id: JobId, job: Job) {
        let video = is_video(&job);
        self.state
            .lock()
            .unwrap()
            .queue
            .push_back(Pending { id, job, video });
        self.pump();
    }

    /// Runs jobs that start from now on with `registry`.
    pub fn set_registry(&self, registry: Arc<Registry>) {
        *self.registry.lock().unwrap() = registry;
    }

    pub fn set_concurrency(&self, n: usize) {
        self.state.lock().unwrap().concurrency = n.max(1);
        self.pump();
    }

    /// Stops a running job, or drops a queued one. Either way the job
    /// finishes as cancelled.
    pub fn cancel(&self, id: JobId) {
        let mut state = self.state.lock().unwrap();
        if let Some(cancel) = state.running.get(&id) {
            cancel.cancel();
        } else if let Some(i) = state.queue.iter().position(|p| p.id == id) {
            state.queue.remove(i);
            drop(state);
            let _ = self
                .tx
                .unbounded_send(Update::Finished(id, Err((&Error::Cancelled).into())));
        }
    }

    /// Starts as many queued jobs as the limits allow.
    fn pump(&self) {
        let mut state = self.state.lock().unwrap();
        while state.running.len() < state.concurrency {
            let video_busy = state.video_running;
            let Some(i) = state.queue.iter().position(|p| !(p.video && video_busy)) else {
                break;
            };
            let Pending { id, job, video } = state.queue.remove(i).expect("index from position");
            let cancel = Cancel::new();
            state.running.insert(id, cancel.clone());
            state.video_running |= video;
            let this = self.clone();
            std::thread::Builder::new()
                .name(format!("convt-job-{id}"))
                .spawn(move || this.work(id, job, video, cancel))
                .expect("spawn job thread");
        }
    }

    fn work(&self, id: JobId, job: Job, video: bool, cancel: Cancel) {
        let tx = &self.tx;
        let _ = tx.unbounded_send(Update::Started(id));
        let registry = self.registry.lock().unwrap().clone();
        let result = registry
            .run(
                &job,
                &|p| drop(tx.unbounded_send(Update::Progress(id, p))),
                &cancel,
            )
            .map_err(|e| JobError::from(&e));
        {
            let mut state = self.state.lock().unwrap();
            state.running.remove(&id);
            if video {
                state.video_running = false;
            }
        }
        let _ = tx.unbounded_send(Update::Finished(id, result));
        self.pump();
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Queued,
    Running(Option<f32>),
    Done(Vec<PathBuf>),
    Failed(JobError),
    Cancelled,
}

impl Status {
    pub fn is_finished(&self) -> bool {
        matches!(self, Self::Done(_) | Self::Failed(_) | Self::Cancelled)
    }
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub id: JobId,
    pub input: PathBuf,
    pub to: &'static Format,
    pub status: Status,
    /// The options and output it was queued with, for Retry.
    pub setup: Setup,
    /// When the job started running.
    pub started: Option<Instant>,
}

impl Entry {
    /// A rough estimate of the time left, from the progress so far. `None`
    /// until there is enough progress to say.
    pub fn remaining(&self, now: Instant) -> Option<Duration> {
        let Status::Running(Some(p)) = self.status else {
            return None;
        };
        let elapsed = now.checked_duration_since(self.started?)?;
        (0.05..1.0)
            .contains(&p)
            .then(|| elapsed.mul_f32((1.0 - p) / p))
    }
}

/// Every job this session, oldest first.
#[derive(Default)]
pub struct Queue {
    pub entries: Vec<Entry>,
    next_id: JobId,
}

impl Queue {
    /// Adds a job and returns the id to submit it under.
    pub fn add(&mut self, job: &Job) -> JobId {
        self.next_id += 1;
        self.entries.push(Entry {
            id: self.next_id,
            input: job.input.clone(),
            to: job.to,
            status: Status::Queued,
            setup: Setup {
                options: job.options.clone(),
                output: job.output.clone(),
            },
            started: None,
        });
        self.next_id
    }

    pub fn get(&self, id: JobId) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// Applies an update. Returns the entry when the update finished it.
    pub fn apply(&mut self, update: Update) -> Option<&Entry> {
        let (id, status) = match update {
            Update::Started(id) => (id, Status::Running(None)),
            Update::Progress(id, p) => (id, Status::Running(p)),
            Update::Finished(id, Ok(outputs)) => (id, Status::Done(outputs)),
            Update::Finished(id, Err(e)) if e.kind == "cancelled" => (id, Status::Cancelled),
            Update::Finished(id, Err(e)) => (id, Status::Failed(e)),
        };
        let entry = self.entries.iter_mut().find(|e| e.id == id)?;
        // A late progress report must not undo a finish.
        if entry.status.is_finished() {
            return None;
        }
        let finished = status.is_finished();
        if matches!(status, Status::Running(_)) && entry.started.is_none() {
            entry.started = Some(Instant::now());
        }
        entry.status = status;
        finished.then_some(&*entry)
    }

    pub fn active(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| !e.status.is_finished())
            .count()
    }

    /// "1 converting · 2 waiting", or `None` when nothing is left to do.
    pub fn progress_line(&self) -> Option<String> {
        let running = self
            .entries
            .iter()
            .filter(|e| matches!(e.status, Status::Running(_)))
            .count();
        match (running, self.active() - running) {
            (0, 0) => None,
            (r, 0) => Some(format!("{r} converting")),
            (0, w) => Some(format!("{w} waiting")),
            (r, w) => Some(format!("{r} converting · {w} waiting")),
        }
    }

    /// Forgets finished jobs.
    pub fn clear_finished(&mut self) {
        self.entries.retain(|e| !e.status.is_finished());
    }
}

#[cfg(test)]
mod tests {
    use convt_core::{Ctx, Engine, Step, format_by_id};
    use futures::StreamExt;
    use std::path::Path;

    use super::*;

    /// Copies the input after waiting, checking for cancellation.
    struct Slow;

    impl Engine for Slow {
        fn id(&self) -> &'static str {
            "slow"
        }
        fn steps(&self) -> Vec<Step> {
            let f = |id| format_by_id(id).unwrap();
            vec![
                Step {
                    from: f("png"),
                    to: f("jpeg"),
                },
                Step {
                    from: f("mp4"),
                    to: f("webm"),
                },
            ]
        }
        fn convert(
            &self,
            ctx: &Ctx,
            input: &Path,
            out_dir: &Path,
        ) -> convt_core::Result<Vec<PathBuf>> {
            for i in 0..20 {
                ctx.check()?;
                ctx.progress(i as f32 / 20.0);
                std::thread::sleep(Duration::from_millis(10));
            }
            let out = ctx.artifact(out_dir, 0);
            std::fs::copy(input, &out)?;
            Ok(vec![out])
        }
    }

    fn setup(concurrency: usize) -> (Runner, UnboundedReceiver<Update>, tempfile::TempDir) {
        let mut registry = Registry::new();
        registry.register(Arc::new(Slow));
        let dir = tempfile::tempdir().unwrap();
        for name in ["a.png", "b.png", "c.mp4", "d.mp4"] {
            std::fs::write(dir.path().join(name), name).unwrap();
        }
        let (runner, rx) = Runner::new(Arc::new(registry), concurrency);
        (runner, rx, dir)
    }

    fn drain(
        rx: &mut UnboundedReceiver<Update>,
        queue: &mut Queue,
        until_done: usize,
    ) -> Vec<Update> {
        let mut log = Vec::new();
        futures::executor::block_on(async {
            let mut done = 0;
            while done < until_done {
                let u = rx.next().await.expect("runner alive");
                log.push(u.clone());
                if queue.apply(u).is_some() {
                    done += 1;
                }
            }
        });
        log
    }

    #[test]
    fn runs_cancels_and_reports() {
        let (runner, mut rx, dir) = setup(1);
        let mut queue = Queue::default();
        let f = |id| format_by_id(id).unwrap();
        let jobs = [
            Job::new(dir.path().join("a.png"), f("jpeg")),
            Job::new(dir.path().join("b.png"), f("jpeg")),
            Job::new(dir.path().join("missing.png"), f("jpeg")),
        ];
        let ids: Vec<JobId> = jobs.iter().map(|j| queue.add(j)).collect();
        for (id, job) in ids.iter().zip(jobs) {
            runner.submit(*id, job);
        }
        // With one slot, b is still queued and is dropped without starting.
        runner.cancel(ids[1]);
        let log = drain(&mut rx, &mut queue, 3);
        assert!(!log.contains(&Update::Started(ids[1])));
        let status = |i: usize| &queue.get(ids[i]).unwrap().status;
        let Status::Done(out) = status(0) else {
            panic!("{:?}", status(0))
        };
        assert_eq!(std::fs::read(&out[0]).unwrap(), b"a.png");
        assert_eq!(status(1), &Status::Cancelled);
        assert!(matches!(status(2), Status::Failed(e) if e.kind == "io"));
        assert_eq!(queue.active(), 0);
        queue.clear_finished();
        assert!(queue.entries.is_empty());
    }

    #[test]
    fn one_video_at_a_time() {
        let (runner, mut rx, dir) = setup(4);
        let mut queue = Queue::default();
        let f = |id| format_by_id(id).unwrap();
        let jobs = [
            Job::new(dir.path().join("c.mp4"), f("webm")),
            Job::new(dir.path().join("d.mp4"), f("webm")),
            Job::new(dir.path().join("a.png"), f("jpeg")),
        ];
        let ids: Vec<JobId> = jobs.iter().map(|j| queue.add(j)).collect();
        for (id, job) in ids.iter().zip(jobs) {
            runner.submit(*id, job);
        }
        let log = drain(&mut rx, &mut queue, 3);
        let pos = |u: &Update| log.iter().position(|x| x == u).unwrap();
        let first_done = log
            .iter()
            .position(|u| matches!(u, Update::Finished(id, _) if *id == ids[0]))
            .unwrap();
        // The second video waits for the first; the image doesn't.
        assert!(pos(&Update::Started(ids[1])) > first_done);
        assert!(pos(&Update::Started(ids[2])) < first_done);
    }

    #[test]
    fn cancelling_a_running_job() {
        let (runner, mut rx, dir) = setup(2);
        let mut queue = Queue::default();
        let job = Job::new(dir.path().join("a.png"), format_by_id("jpeg").unwrap());
        let id = queue.add(&job);
        runner.submit(id, job);
        futures::executor::block_on(async {
            while rx.next().await != Some(Update::Started(id)) {}
        });
        runner.cancel(id);
        drain(&mut rx, &mut queue, 1);
        assert_eq!(queue.get(id).unwrap().status, Status::Cancelled);
        assert!(!dir.path().join("a.jpg").exists());
    }

    #[test]
    fn progress_line_counts_running_and_waiting() {
        let mut queue = Queue::default();
        let job = Job::new(PathBuf::from("a.png"), format_by_id("jpeg").unwrap());
        let ids: Vec<JobId> = (0..3).map(|_| queue.add(&job)).collect();
        assert_eq!(queue.progress_line().as_deref(), Some("3 waiting"));
        queue.apply(Update::Started(ids[0]));
        assert_eq!(
            queue.progress_line().as_deref(),
            Some("1 converting · 2 waiting")
        );
        queue.apply(Update::Started(ids[1]));
        queue.apply(Update::Started(ids[2]));
        assert_eq!(queue.progress_line().as_deref(), Some("3 converting"));
        for id in ids {
            queue.apply(Update::Finished(id, Ok(Vec::new())));
        }
        assert_eq!(queue.progress_line(), None);
    }
}
