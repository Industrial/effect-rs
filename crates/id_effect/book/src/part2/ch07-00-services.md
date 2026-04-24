# Services — The Complete DI Pattern

The previous chapters established the building blocks: Tags (identities), Context (the environment), and Layers (constructors). Now we put them together into the complete *Service* pattern.

A Service in id_effect is the combination of:
1. A **trait** defining the interface
2. A **tag** identifying it in the environment
3. One or more **implementations** (production and test)
4. A **layer** that wires an implementation into the environment

This is the full dependency injection story. By the end of this chapter you'll have a working multi-service application wired entirely at compile time.

The sections [Tokio bridge](./ch07-05-tokio-bridge.md) through [Logging](./ch07-11-logger.md) extend the same pattern to **workspace integration crates**: how you **run** effects on Tokio, swap **platform** and **HTTP** implementations, host handlers under **Axum** / **Tower**, and inject **configuration** and **logging** as services. Read them in order when wiring a production binary.
