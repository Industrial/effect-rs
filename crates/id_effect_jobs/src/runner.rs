//! [`JobRunner`] trait and in-memory FIFO implementation.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use id_effect::compute::{FabricJobSpec, WorkProfile};
use id_effect::{Effect, runtime::run_blocking};

fn work_profile_label(profile: WorkProfile) -> String {
  match profile {
    WorkProfile::CpuIntensive => "cpu_intensive".to_string(),
    WorkProfile::IoBound => "io_bound".to_string(),
    WorkProfile::MemoryHeavy => "memory_heavy".to_string(),
    WorkProfile::Remote => "remote".to_string(),
    WorkProfile::Mixed => "mixed".to_string(),
  }
}

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::JobError;

/// Lifecycle state for a queued job.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobState {
  /// Waiting in the queue.
  Pending,
  /// Currently executing.
  Running,
  /// Finished successfully.
  Completed,
  /// Handler returned a typed failure.
  Failed,
}

/// A unit of background work.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobSpec {
  /// Stable job id (assigned on enqueue when empty).
  pub id: String,
  /// Logical job name (handler dispatch key).
  pub name: String,
  /// Opaque payload bytes.
  pub payload: Vec<u8>,
  /// Optional Compute Fabric work profile label for cluster placement.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub work_profile: Option<String>,
  /// Optional target node from placement scoring.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub target_node: Option<String>,
}

impl JobSpec {
  /// Build a job with an auto-generated id.
  pub fn new(name: impl Into<String>, payload: impl Into<Vec<u8>>) -> Self {
    Self {
      id: Uuid::new_v4().to_string(),
      name: name.into(),
      payload: payload.into(),
      work_profile: None,
      target_node: None,
    }
  }

  /// Build a job from a Compute Fabric scale-out payload.
  pub fn from_fabric(spec: FabricJobSpec) -> Self {
    Self {
      id: Uuid::new_v4().to_string(),
      name: spec.name,
      payload: spec.payload,
      work_profile: Some(work_profile_label(spec.work_profile)),
      target_node: spec.target_node,
    }
  }
}

/// Stored job with lifecycle metadata.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobRecord {
  /// The work specification.
  pub spec: JobSpec,
  /// Current lifecycle state.
  pub state: JobState,
}

/// Optional runner hook for Compute Fabric cluster offload (stub for Kafka/Apalis wiring).
#[allow(dead_code)]
pub trait FabricJobRunner: JobRunner {
  /// Enqueue a fabric-produced scale-out job.
  fn enqueue_fabric(&self, spec: FabricJobSpec) -> Effect<JobRecord, JobError, ()> {
    self.enqueue(JobSpec::from_fabric(spec))
  }
}

/// Background job queue abstraction.
pub trait JobRunner: Send + Sync {
  /// Enqueue a job; returns the stored record (with assigned id).
  fn enqueue(&self, spec: JobSpec) -> Effect<JobRecord, JobError, ()>;

  /// Dequeue the next pending job, marking it running. Returns `None` when empty.
  fn dequeue(&self) -> Effect<Option<JobRecord>, JobError, ()>;

  /// Mark a running job completed.
  fn complete(&self, job_id: &str) -> Effect<(), JobError, ()>;

  /// Mark a running job failed.
  fn fail(&self, job_id: &str) -> Effect<(), JobError, ()>;

  /// Count jobs still pending.
  fn pending_count(&self) -> Effect<usize, JobError, ()>;
}

#[derive(Clone)]
struct MemoryJobRunnerInner {
  queue: Arc<Mutex<VecDeque<JobRecord>>>,
  running: Arc<Mutex<std::collections::HashMap<String, JobRecord>>>,
}

/// In-memory FIFO job runner (single-process; mutex-backed).
#[cfg(feature = "memory")]
#[derive(Clone)]
pub struct MemoryJobRunner {
  inner: MemoryJobRunnerInner,
}

#[cfg(feature = "memory")]
impl Default for MemoryJobRunner {
  fn default() -> Self {
    Self::new()
  }
}

#[cfg(feature = "memory")]
impl MemoryJobRunner {
  /// Empty queue.
  pub fn new() -> Self {
    Self {
      inner: MemoryJobRunnerInner {
        queue: Arc::new(Mutex::new(VecDeque::new())),
        running: Arc::new(Mutex::new(std::collections::HashMap::new())),
      },
    }
  }
}

#[cfg(feature = "memory")]
impl JobRunner for MemoryJobRunner {
  fn enqueue(&self, mut spec: JobSpec) -> Effect<JobRecord, JobError, ()> {
    if spec.id.is_empty() {
      spec.id = Uuid::new_v4().to_string();
    }
    let record = JobRecord {
      spec,
      state: JobState::Pending,
    };
    let queue = Arc::clone(&self.inner.queue);
    let stored = record.clone();
    Effect::new(move |_r| {
      let mut guard = queue.lock().map_err(|e| JobError::Lock(e.to_string()))?;
      guard.push_back(stored);
      Ok(record)
    })
  }

  fn dequeue(&self) -> Effect<Option<JobRecord>, JobError, ()> {
    let queue = Arc::clone(&self.inner.queue);
    let running = Arc::clone(&self.inner.running);
    Effect::new(move |_r| {
      let mut guard = queue.lock().map_err(|e| JobError::Lock(e.to_string()))?;
      let Some(mut record) = guard.pop_front() else {
        return Ok(None);
      };
      record.state = JobState::Running;
      let mut run_guard = running.lock().map_err(|e| JobError::Lock(e.to_string()))?;
      run_guard.insert(record.spec.id.clone(), record.clone());
      Ok(Some(record))
    })
  }

  fn complete(&self, job_id: &str) -> Effect<(), JobError, ()> {
    let job_id = job_id.to_owned();
    let running = Arc::clone(&self.inner.running);
    Effect::new(move |_r| {
      let mut guard = running.lock().map_err(|e| JobError::Lock(e.to_string()))?;
      let Some(mut record) = guard.remove(&job_id) else {
        return Err(JobError::NotFound(job_id));
      };
      record.state = JobState::Completed;
      Ok(())
    })
  }

  fn fail(&self, job_id: &str) -> Effect<(), JobError, ()> {
    let job_id = job_id.to_owned();
    let running = Arc::clone(&self.inner.running);
    Effect::new(move |_r| {
      let mut guard = running.lock().map_err(|e| JobError::Lock(e.to_string()))?;
      let Some(mut record) = guard.remove(&job_id) else {
        return Err(JobError::NotFound(job_id));
      };
      record.state = JobState::Failed;
      Ok(())
    })
  }

  fn pending_count(&self) -> Effect<usize, JobError, ()> {
    let queue = Arc::clone(&self.inner.queue);
    Effect::new(move |_r| {
      let guard = queue.lock().map_err(|e| JobError::Lock(e.to_string()))?;
      Ok(guard.len())
    })
  }
}

#[cfg(feature = "memory")]
/// Drain up to `limit` pending jobs, running `handler` for each.
///
/// Returns the number of jobs processed. Handler failures mark the job failed but do not
/// stop the drain loop.
pub fn drain_jobs<R, H>(runner: R, limit: usize, handler: H) -> Effect<usize, JobError, ()>
where
  R: JobRunner + 'static,
  H: Fn(&JobSpec) -> Effect<(), JobError, ()> + Copy + 'static,
{
  Effect::new(move |_r| {
    let mut processed = 0usize;
    for _ in 0..limit {
      let maybe = run_blocking(runner.dequeue(), ())?;
      let Some(record) = maybe else {
        break;
      };
      let id = record.spec.id.clone();
      match run_blocking(handler(&record.spec), ()) {
        Ok(()) => run_blocking(runner.complete(&id), ())?,
        Err(_) => run_blocking(runner.fail(&id), ())?,
      }
      processed += 1;
    }
    Ok(processed)
  })
}

#[cfg(all(test, feature = "memory"))]
mod tests {
  use super::*;
  use id_effect::{fail, succeed};

  #[test]
  fn enqueue_and_dequeue_fifo() {
    let runner = MemoryJobRunner::new();
    let a = run_blocking(runner.enqueue(JobSpec::new("a", b"1".as_slice())), ()).unwrap();
    let b = run_blocking(runner.enqueue(JobSpec::new("b", b"2".as_slice())), ()).unwrap();
    assert_ne!(a.spec.id, b.spec.id);

    let first = run_blocking(runner.dequeue(), ()).unwrap().unwrap();
    assert_eq!(first.spec.name, "a");
    assert_eq!(first.state, JobState::Running);

    run_blocking(runner.complete(&first.spec.id), ()).unwrap();

    let second = run_blocking(runner.dequeue(), ()).unwrap().unwrap();
    assert_eq!(second.spec.name, "b");
  }

  #[test]
  fn pending_count_tracks_queue() {
    let runner = MemoryJobRunner::new();
    run_blocking(runner.enqueue(JobSpec::new("x", vec![])), ()).unwrap();
    assert_eq!(run_blocking(runner.pending_count(), ()).unwrap(), 1);
    run_blocking(runner.dequeue(), ()).unwrap();
    assert_eq!(run_blocking(runner.pending_count(), ()).unwrap(), 0);
  }

  #[test]
  fn drain_jobs_runs_handler() {
    let runner = MemoryJobRunner::new();
    run_blocking(runner.enqueue(JobSpec::new("work", b"payload")), ()).unwrap();
    let n = run_blocking(
      drain_jobs(runner.clone(), 10, |spec| {
        assert_eq!(spec.name, "work");
        succeed::<(), JobError, ()>(())
      }),
      (),
    )
    .unwrap();
    assert_eq!(n, 1);
    assert_eq!(run_blocking(runner.pending_count(), ()).unwrap(), 0);
  }

  #[test]
  fn drain_jobs_marks_failures() {
    let runner = MemoryJobRunner::new();
    let rec = run_blocking(runner.enqueue(JobSpec::new("bad", vec![])), ()).unwrap();
    let n = run_blocking(
      drain_jobs(runner.clone(), 1, |_spec| {
        fail::<(), JobError, ()>(JobError::NotFound("x".into()))
      }),
      (),
    )
    .unwrap();
    assert_eq!(n, 1);
    assert!(run_blocking(runner.complete(&rec.spec.id), ()).is_err());
  }

  #[test]
  fn dequeue_empty_returns_none() {
    let runner = MemoryJobRunner::new();
    assert!(run_blocking(runner.dequeue(), ()).unwrap().is_none());
  }

  #[test]
  fn enqueue_preserves_provided_id() {
    let runner = MemoryJobRunner::new();
    let spec = JobSpec {
      id: "fixed-id".into(),
      name: "job".into(),
      payload: b"x".to_vec(),
      work_profile: None,
      target_node: None,
    };
    let rec = run_blocking(runner.enqueue(spec), ()).unwrap();
    assert_eq!(rec.spec.id, "fixed-id");
  }

  #[test]
  fn fail_marks_running_job_failed() {
    let runner = MemoryJobRunner::new();
    run_blocking(runner.enqueue(JobSpec::new("j", vec![])), ()).unwrap();
    let running = run_blocking(runner.dequeue(), ()).unwrap().unwrap();
    run_blocking(runner.fail(&running.spec.id), ()).unwrap();
    assert!(run_blocking(runner.complete(&running.spec.id), ()).is_err());
  }
}
