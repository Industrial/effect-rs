//! Ex 106 — `TokioRuntime` implements `id_effect::Runtime` (sleep / yield on the Tokio driver).
//!
//! Run: `cargo run -p id_effect_tokio --example 106_tokio_runtime`

use id_effect::{Runtime, run_async, succeed};
use id_effect_tokio::{TokioRuntime, yield_now};
use std::time::Duration;

fn main() {
  let rt = TokioRuntime::new_current_thread().expect("tokio runtime should build");
  rt.block_on(async {
    assert_eq!(
      run_async(rt.sleep(Duration::from_millis(0)), ()).await,
      Ok(())
    );
    assert_eq!(run_async(yield_now(&rt), ()).await, Ok(()));
    assert_eq!(run_async(succeed::<u8, (), ()>(42), ()).await, Ok(42));
  });
  println!("106_tokio_runtime ok");
}
