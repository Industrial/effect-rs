# Why Effects?

Before we write a single line of effect code, we need to understand the problem we're solving.

This chapter isn't about effect-rs yet. It's about the pain you've already felt — the pain that led you to pick up this book. We're going to name that pain, examine it, and understand its root cause.

By the end of this chapter, you'll know:

- Why async Rust often feels harder than it should
- What an Effect actually is (spoiler: it's simpler than you think)
- Why the `Effect<A, E, R>` type signature contains three letters, not two
- Why laziness isn't a limitation — it's a superpower

Let's start with the jungle.
