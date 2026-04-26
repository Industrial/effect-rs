//! Ex 001 — Durable workflow restart: completed steps are **skipped** on replay.
//!
//! Simulates a 3-step workflow that "crashes" after step 1, then restarts and
//! skips the already-completed steps instead of re-running them.
//!
//! Run: `cargo run -p id_effect_workflow --example 001_restart_resume`

use id_effect_workflow::{DurableWorkflowLog, WorkflowError};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

fn main() -> Result<(), WorkflowError> {
  // Use a real on-disk database so the restart simulation is meaningful.
  let db_path = std::env::temp_dir().join("id_effect_workflow_001_demo.db");
  // Start fresh for a clean demo run.
  let _ = std::fs::remove_file(&db_path);

  let run_count = Arc::new(AtomicU32::new(0));

  // ── First run: complete step 0 and step 1, then "crash" before step 2 ──
  {
    let rc = run_count.clone();
    let mut log = DurableWorkflowLog::open(&db_path)?;
    log.register_workflow("order-wf-42")?;

    let validated: String = log.run_step_typed("order-wf-42", 0, "validate-order", || {
      rc.fetch_add(1, Ordering::SeqCst);
      println!("  [step 0] running validate-order…");
      Ok("order-ok".to_string())
    })?;
    println!("  [step 0] result = {:?}", validated);

    let reserved: u32 = log.run_step_typed("order-wf-42", 1, "reserve-inventory", || {
      rc.fetch_add(1, Ordering::SeqCst);
      println!("  [step 1] running reserve-inventory…");
      Ok(99_u32) // quantity reserved
    })?;
    println!("  [step 1] result = {}", reserved);

    println!("  … simulating crash before step 2 …");
    // `log` is dropped here — connection closed, simulating process exit.
  }
  println!(
    "First run: {} step(s) actually executed\n",
    run_count.load(Ordering::SeqCst)
  );

  // ── Restart: step 0 and step 1 must be replayed from the log, not re-run ──
  {
    let rc = run_count.clone();
    let mut log = DurableWorkflowLog::open(&db_path)?;
    // Note: workflow already registered — do NOT call register_workflow again.

    let validated: String = log.run_step_typed("order-wf-42", 0, "validate-order", || {
      rc.fetch_add(1, Ordering::SeqCst);
      println!("  [step 0] BUG: should not execute on restart!");
      Ok("WRONG".to_string())
    })?;
    println!("  [step 0] replayed = {:?} (no re-execution)", validated);
    assert_eq!(validated, "order-ok");

    let reserved: u32 = log.run_step_typed("order-wf-42", 1, "reserve-inventory", || {
      rc.fetch_add(1, Ordering::SeqCst);
      println!("  [step 1] BUG: should not execute on restart!");
      Ok(0_u32)
    })?;
    println!("  [step 1] replayed = {} (no re-execution)", reserved);
    assert_eq!(reserved, 99);

    let charged: String = log.run_step_typed("order-wf-42", 2, "charge-payment", || {
      rc.fetch_add(1, Ordering::SeqCst);
      println!("  [step 2] running charge-payment (first time)…");
      Ok("payment-ok".to_string())
    })?;
    println!("  [step 2] result = {:?}", charged);
  }
  println!(
    "After restart: {} total step execution(s) across both runs",
    run_count.load(Ordering::SeqCst)
  );

  // Total executions: 2 (first run) + 1 (only new step 2 on restart) = 3
  assert_eq!(run_count.load(Ordering::SeqCst), 3);

  // Clean up temp file.
  let _ = std::fs::remove_file(&db_path);

  println!("\n001_restart_resume ok");
  Ok(())
}
