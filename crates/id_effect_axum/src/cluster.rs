//! Axum mounts for Process Mesh control plane (optional `cluster` feature).

use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use id_effect_node::{ClusterStatus, DrainRequest, RebalanceRequest};

/// Mutable status + admin hooks.
#[derive(Clone)]
pub struct ClusterControl {
  status: Arc<Mutex<ClusterStatus>>,
  on_drain: Arc<dyn Fn(DrainRequest) + Send + Sync>,
  on_rebalance: Arc<dyn Fn(RebalanceRequest) + Send + Sync>,
}

impl ClusterControl {
  /// Snapshot-only control (admin actions are no-ops).
  pub fn snapshot(status: ClusterStatus) -> Self {
    Self {
      status: Arc::new(Mutex::new(status)),
      on_drain: Arc::new(|_| {}),
      on_rebalance: Arc::new(|_| {}),
    }
  }

  /// Full control with callbacks.
  pub fn with_hooks(
    status: ClusterStatus,
    on_drain: impl Fn(DrainRequest) + Send + Sync + 'static,
    on_rebalance: impl Fn(RebalanceRequest) + Send + Sync + 'static,
  ) -> Self {
    Self {
      status: Arc::new(Mutex::new(status)),
      on_drain: Arc::new(on_drain),
      on_rebalance: Arc::new(on_rebalance),
    }
  }

  /// Replace the published status snapshot.
  pub fn set_status(&self, status: ClusterStatus) {
    *self.status.lock().expect("status") = status;
  }

  /// Router: `/cluster/status`, `/cluster/members`, `/cluster/processes`,
  /// `/cluster/drain`, `/cluster/rebalance`.
  pub fn router(self) -> Router {
    Router::new()
      .route("/cluster/status", get(status))
      .route("/cluster/members", get(members))
      .route("/cluster/processes", get(processes))
      .route("/cluster/drain", post(drain))
      .route("/cluster/rebalance", post(rebalance))
      .with_state(self)
  }
}

async fn status(State(ctl): State<ClusterControl>) -> Json<ClusterStatus> {
  Json(ctl.status.lock().expect("status").clone())
}

async fn members(State(ctl): State<ClusterControl>) -> Json<serde_json::Value> {
  let s = ctl.status.lock().expect("status");
  Json(serde_json::json!({ "members": s.members.clone() }))
}

async fn processes(State(ctl): State<ClusterControl>) -> Json<serde_json::Value> {
  let s = ctl.status.lock().expect("status");
  Json(serde_json::json!({ "processes": s.processes.clone() }))
}

async fn drain(
  State(ctl): State<ClusterControl>,
  Json(req): Json<DrainRequest>,
) -> Json<serde_json::Value> {
  {
    let mut s = ctl.status.lock().expect("status");
    s.draining = true;
  }
  (ctl.on_drain)(req);
  Json(serde_json::json!({ "ok": true }))
}

async fn rebalance(
  State(ctl): State<ClusterControl>,
  Json(req): Json<RebalanceRequest>,
) -> Json<serde_json::Value> {
  (ctl.on_rebalance)(req);
  Json(serde_json::json!({ "ok": true }))
}

#[cfg(test)]
mod tests {
  use super::*;
  use axum::body::Body;
  use axum::http::{Request, StatusCode};
  use id_effect_node::MemberView;
  use tower::ServiceExt;

  #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
  async fn status_endpoint_returns_members() {
    let mut st = ClusterStatus::new();
    st.members.push(MemberView {
      name: "a@t".into(),
      incarnation: "1".into(),
      addr: None,
    });
    let app = ClusterControl::snapshot(st).router();
    let res = app
      .oneshot(
        Request::builder()
          .uri("/cluster/status")
          .body(Body::empty())
          .unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
  }
}
