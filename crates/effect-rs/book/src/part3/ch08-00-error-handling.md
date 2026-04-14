# Error Handling — Cause, Exit, and Recovery

Part II gave you the full dependency injection story. Part III is about what happens when things go wrong — and in production, things always go wrong.

Rust's `Result<T, E>` is excellent for *expected* errors: outcomes you anticipated and typed. But real programs also encounter *unexpected* failures: panics, OOM conditions, and cancelled fibers. effect-rs models all of these with a richer type hierarchy.

This chapter introduces `Cause` (the full error taxonomy), `Exit` (the terminal outcome of any effect), and the combinators for recovering from both.
