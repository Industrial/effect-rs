//! Ex 108 — Tokio-backed `Runtime`: monotonic `now`, async `sleep`, cooperative `yield_now`.
//!
//! Run: `cargo run -p id_effect_tokio --example 108_tokio_clock`

use id_effect::Runtime;
use id_effect_tokio::{TokioRuntime, run_async, yield_now};
use std::time::Duration;

fn main() {
  let tokio_rt = tokio::runtime::Builder::new_current_thread()
    .enable_time()
    .build()
    .expect("tokio runtime should build");
  let rt = TokioRuntime::from_handle(tokio_rt.handle().clone());
  tokio_rt.block_on(async {
    let t1 = rt.now();
    let t2 = rt.now();
    assert!(t2 >= t1);

    assert_eq!(
      run_async(rt.sleep(Duration::from_millis(0)), ()).await,
      Ok(())
    );
    assert_eq!(run_async(yield_now(&rt), ()).await, Ok(()));
  });

  println!("108_tokio_clock ok");
}
