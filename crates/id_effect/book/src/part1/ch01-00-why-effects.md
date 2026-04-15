# Why Effects?

Before we write effect code in detail, it helps to agree on **what problem we are solving** and **where id_effect sits** relative to ordinary async Rust.

Rust’s async model is built on `Future` and executors: futures are lazy until polled, and `.await` is how async functions compose. That model is not a mistake—it is the standard way to express non-blocking I/O and concurrency.

At application scale, the difficulties are usually **engineering** ones:

- **Errors** — mapping and aggregating failures across layers without losing structure.
- **Dependencies** — passing clients, configuration, and context without turning every function signature into a long parameter list (or hiding the same behind globals).
- **Concurrency** — knowing who owns a task, how it shuts down, and what happens on cancellation.

This chapter names those patterns, relates them to how `Effect<A, E, R>` is designed, and sets up the rest of Part I.

By the end of the chapter you should understand:

- Why teams reach for a **declarative** layer on top of hand-written `async fn` chains.
- What “effect” means in this book: a **description** of work, separate from **running** it with a chosen environment.
- Why the type has three parameters (`A`, `E`, `R`) and why that matters for APIs and tests.

We start with a concrete look at those recurring challenges.
