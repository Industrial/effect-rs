# Summary

[Introduction](./introduction.md)

# Part I: Foundations

- [Why Effects?](./part1/ch01-00-why-effects.md)
  - [Challenges in Large Async Codebases](./part1/ch01-01-challenges-in-large-async-codebases.md)
  - [What Even Is an Effect?](./part1/ch01-02-what-is-an-effect.md)
  - [The Three Type Parameters](./part1/ch01-03-type-parameters.md)
  - [Laziness as a Superpower](./part1/ch01-04-laziness.md)
- [Your First Effect](./part1/ch02-00-first-effect.md)
  - [Creating Effects](./part1/ch02-01-creating-effects.md)
  - [Transforming Success](./part1/ch02-02-transforming.md)
  - [Chaining Effects](./part1/ch02-03-chaining.md)
  - [Your First Real Program](./part1/ch02-04-real-program.md)
- [The effect! Macro](./part1/ch03-00-effect-macro.md)
  - [Why Do-Notation Exists](./part1/ch03-01-why-do-notation.md)
  - [The ~ Operator Explained](./part1/ch03-02-bind-operator.md)
  - [Error Handling Inside effect!](./part1/ch03-03-error-handling.md)
  - [When Not to Use the Macro](./part1/ch03-04-when-not-to-use.md)

# Part II: Environment & Dependencies

- [The R Parameter](./part2/ch04-00-r-parameter.md)
  - [R Revisited](./part2/ch04-01-r-revisited.md)
  - [Providing Dependencies](./part2/ch04-02-providing.md)
  - [Widening and Narrowing](./part2/ch04-03-widening-narrowing.md)
  - [R as Documentation](./part2/ch04-04-r-as-docs.md)
- [Capability services](./part2/ch05-00-tags-context.md)
  - [The Problem with Positional Types](./part2/ch05-01-positional-problem.md)
  - [Capability services](./part2/ch05-02-tags.md)
  - [`Env`](./part2/ch05-03-context-hlists.md)
  - [Get and GetMut](./part2/ch05-04-get-getmut.md)
- [Providers](./part2/ch06-00-layers.md)
  - [What Is a Provider?](./part2/ch06-01-what-is-layer.md)
  - [Building Providers](./part2/ch06-02-building-layers.md)
  - [Composing Providers](./part2/ch06-03-stacking.md)
  - [Capability Graphs](./part2/ch06-04-layer-graphs.md)
- [Services](./part2/ch07-00-services.md)
  - [Service Traits](./part2/ch07-01-service-traits.md)
  - [Needs and ~Key](./part2/ch07-02-service-env.md)
  - [ProviderSpec](./part2/ch07-03-providing-services.md)
  - [A Complete DI Example](./part2/ch07-04-complete-example.md)
  - [Tokio bridge (`id_effect_tokio`)](./part2/ch07-05-tokio-bridge.md)
  - [Platform I/O (`id_effect_platform`)](./part2/ch07-06-platform-services.md)
  - [HTTP via reqwest (`id_effect_platform::http::reqwest`)](./part2/ch07-07-reqwest-http.md)
  - [Axum host (`id_effect_axum`)](./part2/ch07-08-axum-host.md)
  - [Tower service (`id_effect_tower`)](./part2/ch07-09-tower-service.md)
  - [Configuration (`id_effect_config`)](./part2/ch07-10-config.md)
  - [Logging (`id_effect_logger`)](./part2/ch07-11-logger.md)
  - [RPC boundaries (`id_effect_rpc`)](./part2/ch07-12-rpc-boundaries.md)
  - [Durable workflow spike (`id_effect_workflow`)](./part2/ch07-12-durable-workflow.md)

# Part III: Real Programs

- [Error Handling](./part3/ch08-00-error-handling.md)
  - [Beyond Result](./part3/ch08-01-beyond-result.md)
  - [Exit](./part3/ch08-02-exit.md)
  - [Recovery Combinators](./part3/ch08-03-recovery.md)
  - [Error Accumulation](./part3/ch08-04-accumulation.md)
- [Concurrency & Fibers](./part3/ch09-00-concurrency.md)
  - [What Are Fibers?](./part3/ch09-01-what-are-fibers.md)
  - [Spawning and Joining](./part3/ch09-02-spawning-joining.md)
  - [Cancellation](./part3/ch09-03-cancellation.md)
  - [FiberRef](./part3/ch09-04-fiberref.md)
  - [Supervision](./part3/ch09-05-supervision.md)
- [Resources & Scopes](./part3/ch10-00-resources.md)
  - [The Resource Problem](./part3/ch10-01-resource-problem.md)
  - [Scopes and Finalizers](./part3/ch10-02-scopes-finalizers.md)
  - [acquire_release](./part3/ch10-03-acquire-release.md)
  - [Pools](./part3/ch10-04-pools.md)
- [Scheduling](./part3/ch11-00-scheduling.md)
  - [Schedule](./part3/ch11-01-schedule.md)
  - [Built-in Schedules](./part3/ch11-02-builtin-schedules.md)
  - [retry and repeat](./part3/ch11-03-retry-repeat.md)
- [Compute Fabric](./part3/ch12-00-compute-fabric.md)
  - [Clock Injection](./part3/ch11-04-clock-injection.md)
- [CLI with clap](./part3/ch16-00-cli-with-clap.md)
  - [Exit codes for `main`](./part3/ch16-01-cli-exit-codes.md)
  - [Config + `Secret` from flags](./part3/ch16-02-cli-config-secret.md)

# Part IV: Advanced

- [Software Transactional Memory](./part4/ch12-00-stm.md)
  - [Why STM?](./part4/ch12-01-why-stm.md)
  - [TRef](./part4/ch12-02-tref.md)
  - [Stm and commit](./part4/ch12-03-stm-commit.md)
  - [Transactional Collections](./part4/ch12-04-collections.md)
- [Streams](./part4/ch13-00-streams.md)
  - [Stream vs Effect](./part4/ch13-01-stream-vs-effect.md)
  - [Chunks](./part4/ch13-02-chunks.md)
  - [Backpressure Policies](./part4/ch13-03-backpressure.md)
  - [Sinks](./part4/ch13-04-sinks.md)
  - [Parallelism (Rayon)](./part4/ch13-05-parallelism.md)
- [Schema](./part4/ch14-00-schema.md)
  - [The Unknown Type](./part4/ch14-01-unknown.md)
  - [Schema Combinators](./part4/ch14-02-combinators.md)
  - [Validation and Refinement](./part4/ch14-03-validation.md)
  - [ParseErrors](./part4/ch14-04-parse-errors.md)
- [Testing](./part4/ch15-00-testing.md)
  - [run_test](./part4/ch15-01-run-test.md)
  - [TestClock](./part4/ch15-02-test-clock.md)
  - [Mocking Services](./part4/ch15-03-mocking.md)
  - [Property Testing](./part4/ch15-04-property-testing.md)

# Part V: Functional Patterns

- [Optics (`id_effect_optics`)](./part5/ch18-00-optics.md)
  - [Lenses](./part5/ch18-01-lenses.md)
  - [Prisms and Optionals](./part5/ch18-02-prisms-optionals.md)
  - [Traversals and schema bridge](./part5/ch18-03-traversals-schema.md)

- [State Machines (`id_effect_fsm`)](./part5/ch19-00-state-machines.md)
  - [Transition tables](./part5/ch19-01-transition-tables.md)
  - [Effect interpreter](./part5/ch19-02-effect-interpreter.md)
  - [Sagas and session types](./part5/ch19-03-saga-session.md)
  - [Workflow bridge](./part5/ch19-04-workflow-bridge.md)

- [Parser Combinators](./part5/ch20-00-parser-combinators.md)

- [Advanced Streaming](./part5/ch22-00-advanced-streaming.md)
  - [Windowing](./part5/ch22-01-windowing.md)
  - [Stream joins](./part5/ch22-02-joins.md)
  - [Replay fanout](./part5/ch22-03-replay-fanout.md)
  - [state_scan FSM stepping](./part5/ch22-04-state-scan.md)
  - [Transducers on streams](./part5/ch22-05-transducers.md)

- [Runtime Resilience](./part5/ch21-00-runtime-resilience.md)

- [Verification and Metaprogramming](./part5/ch24-00-verification-and-macros.md)

- [Events and projections (`id_effect_events`)](./part5/ch23-00-events-and-projections.md)

# Part VI: Application Platform

- [Platform introduction](./part6/ch26-00-platform-introduction.md)
- [Observability and health](./part6/ch27-00-observability.md)
- [Data access (`id_effect_sql`)](./part6/ch28-00-data.md)
- [API boundaries (`id_effect_rpc`)](./part6/ch29-00-api-boundaries.md)
- [Application host](./part6/ch30-00-application.md)
- [Async messaging and jobs](./part6/ch31-00-async-messaging.md)
- [Workflow and cluster](./part6/ch32-00-workflow-cluster.md)
- [DX, generators, and deploy](./part6/ch34-00-dx-ship.md)
- [AI and MCP (`id_effect_ai`)](./part6/ch35-00-ai.md)

# Part VII: Process Mesh

- [Process Mesh overview](./part7/ch36-00-process-mesh.md)
- [Processes and supervision](./part7/ch37-00-processes-supervision.md)
- [Distribution and membership](./part7/ch38-00-distribution.md)
- [Cluster groups and takeover](./part7/ch39-00-cluster-supervision.md)

# Appendices

- [API Quick Reference](./appendix-a-api-reference.md)
- [Migrating from `async fn` to effects](./appendix-b-migration.md)
- [Glossary](./appendix-c-glossary.md)
- [Workspace tooling (macros and lints)](./appendix-d-workspace-tooling.md)

