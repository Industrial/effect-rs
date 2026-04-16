//! Ex 109 — End-to-end: Tokio [`TokioRuntime`], typed [`req!`] context, `effect!`, streams, and `catch`.
//!
//! Run: `cargo run -p id_effect_tokio --example 109_tokio_end_to_end`

use id_effect::{Effect, Runtime, Skip1, Skip2, Stream, ctx, effect, req, run_async, succeed};
use id_effect_tokio::{TokioRuntime, yield_now};
use std::time::Duration;

id_effect::service_key!(struct ApiBaseUrlKey);
id_effect::service_key!(struct ApiTokenKey);
id_effect::service_key!(struct MinPriceKey);

type Env = req!(ApiBaseUrlKey: &'static str | ApiTokenKey: &'static str | MinPriceKey: f64);

#[derive(Debug, Clone, PartialEq)]
struct Quote {
  symbol: &'static str,
  price: f64,
}

#[derive(Debug, Clone, PartialEq)]
struct Report {
  kept_quotes: usize,
  total_notional: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AppError {
  MissingApiToken,
}

fn fetch_quotes_async() -> Effect<Vec<Quote>, AppError, Env> {
  Effect::new_async(|r: &mut Env| {
    Box::pin(async move {
      let _api_base_url = *r.get::<ApiBaseUrlKey>();
      let token = *r.get_path::<ApiTokenKey, Skip1>();
      if token.is_empty() {
        return Err(AppError::MissingApiToken);
      }

      core::future::ready(()).await;

      Ok(vec![
        Quote {
          symbol: "SOL",
          price: 190.0,
        },
        Quote {
          symbol: "BTC",
          price: 64_000.0,
        },
        Quote {
          symbol: "BONK",
          price: 0.000_03,
        },
      ])
    })
  })
}

fn market_report() -> Effect<Report, AppError, Env> {
  effect!(|r: &mut Env| {
    let min_price = ~Ok::<f64, AppError>(*r.get_path::<MinPriceKey, Skip2>());

    let filtered: Vec<Quote> = ~Stream::from_effect(fetch_quotes_async())
      .filter(Box::new(move |q: &Quote| q.price >= min_price))
      .run_collect();

    let kept = ~Ok::<usize, AppError>(filtered.len());

    let total_notional = ~Stream::from_effect(succeed::<Vec<Quote>, AppError, Env>(filtered))
      .map(|q| q.price)
      .run_fold(0.0_f64, |acc, px| acc + px);

    Report {
      kept_quotes: kept,
      total_notional,
    }
  })
}

fn main() {
  let tokio_rt = tokio::runtime::Builder::new_current_thread()
    .enable_time()
    .build()
    .expect("tokio runtime should build");
  let rt = TokioRuntime::from_handle(tokio_rt.handle().clone());
  tokio_rt.block_on(async {
    let t1 = rt.now();
    assert_eq!(run_async(yield_now(&rt), ()).await, Ok(()));
    assert_eq!(
      run_async(rt.sleep(Duration::from_millis(0)), ()).await,
      Ok(())
    );
    assert!(rt.now() >= t1);
  });

  let env_ok = ctx!(
    ApiBaseUrlKey => "https://api.exchange.local",
    ApiTokenKey => "secret-token",
    MinPriceKey => 1.0_f64,
  );

  let report_ok = pollster::block_on(run_async(market_report(), env_ok));
  assert_eq!(
    report_ok,
    Ok(Report {
      kept_quotes: 2,
      total_notional: 64_190.0,
    })
  );

  let env_missing_token = ctx!(
    ApiBaseUrlKey => "https://api.exchange.local",
    ApiTokenKey => "",
    MinPriceKey => 1.0_f64,
  );
  let recovered = pollster::block_on(run_async(
    market_report().catch(|err| match err {
      AppError::MissingApiToken => succeed::<Report, AppError, Env>(Report {
        kept_quotes: 0,
        total_notional: 0.0,
      }),
    }),
    env_missing_token,
  ));
  assert_eq!(
    recovered,
    Ok(Report {
      kept_quotes: 0,
      total_notional: 0.0,
    })
  );

  println!("109_tokio_end_to_end ok");
}
