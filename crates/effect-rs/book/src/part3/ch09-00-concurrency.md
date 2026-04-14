# Concurrency & Fibers — Structured Async

Async Rust gives you the ability to do many things concurrently. The challenge is doing it *safely* — without fire-and-forget tasks that outlive their parent, without silent failures when a task panics, and without resource leaks when tasks are cancelled.

effect-rs uses **Fibers** for structured concurrency. A Fiber is a lightweight, interruptible async task with a typed result, an explicit lifecycle, and guaranteed cleanup.

This chapter covers spawning fibers, joining them, cancelling them gracefully, and using `FiberRef` for fiber-local state.
