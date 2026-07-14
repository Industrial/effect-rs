//! Gated integration test for the Postgres advisory-lock `SingletonLease`.
//!
//! Runs only when built with `--features pg` **and** `IDE_PG_TEST_URL` points at a reachable
//! Postgres (e.g. `postgres://user:pass@localhost/db`). Without the URL the test is a no-op so
//! CI without a database stays green.

#![cfg(feature = "pg")]

use std::time::Duration;

use id_effect_node::singleton::{PgAdvisoryLease, SingletonError, SingletonLease};

fn test_url() -> Option<String> {
  std::env::var("IDE_PG_TEST_URL")
    .ok()
    .filter(|url| !url.is_empty())
}

#[test]
fn pg_advisory_lease_enforces_single_holder() {
  let Some(url) = test_url() else {
    eprintln!("skipping: set IDE_PG_TEST_URL to run the Postgres advisory-lock test");
    return;
  };
  let ttl = Duration::from_secs(30);
  let name = format!("id_effect-it-{}", std::process::id());

  let node_a = PgAdvisoryLease::connect(&url).expect("node a connects");
  let node_b = PgAdvisoryLease::connect(&url).expect("node b connects");

  // First contender wins the advisory lock.
  node_a.try_acquire(&name, "a", ttl).expect("a acquires");
  assert_eq!(node_a.holder(&name).as_deref(), Some("a"));

  // Second contender on a separate session is refused.
  assert!(matches!(
    node_b.try_acquire(&name, "b", ttl),
    Err(SingletonError::HeldBy(_))
  ));

  // Heartbeat succeeds while the session is alive, then the holder steps down.
  node_a.renew(&name, "a", ttl).expect("a renews");
  node_a.release(&name, "a").expect("a releases");

  // After release the lock is available to the other node.
  node_b
    .try_acquire(&name, "b", ttl)
    .expect("b acquires after release");
  assert_eq!(node_b.holder(&name).as_deref(), Some("b"));
  node_b.release(&name, "b").expect("b releases");
}
