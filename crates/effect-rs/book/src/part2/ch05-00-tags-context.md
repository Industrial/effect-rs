# Tags and Context — Compile-Time Service Lookup

Chapter 4 showed how `R` encodes dependencies as types. We used simple types like `Database` and `Logger`. That works for small programs, but breaks down as the dependency graph grows.

This chapter introduces the solution: **Tags**. Tags give values compile-time identities, and **Context** assembles them into a heterogeneous list that the compiler can query by name, not by position.

By the end you'll understand why effect-rs uses this structure instead of tuples, and how to extract exactly the service you need from any environment.
