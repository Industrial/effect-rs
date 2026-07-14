# ADR 0009 — Process Mesh (BEAM-grade processes, supervision trees, and distribution)

## Status

Accepted

## Context

id_effect 0.4.0 delivered a high-quality **intra-node** concurrency core:

- Structured lifetime: `Scope` (LIFO finalizers, hierarchical close) and
  `CancellationToken` (child-token cascade).
- Restart supervision: `concurrency::Supervisor` + `SupervisorPolicy`
  (Terminate / Restart / RestartWithLimit / Escalate / Ignore) driven by
  `Schedule` on an injected `Clock`.
- A runtime seam: the `Runtime` trait (`spawn_with` takes a `Send` factory
  returning `(Effect, R)`), implemented by `ThreadSleepRuntime` and
  `TokioRuntime`.
- A resource-governance control plane: Compute Fabric
  (telemetry → admission → placement → rebalance) with `WorkProfile`,
  `RebalanceStrategy::ScaleOut`, and the `ClusterResourcePolicy` /
  `FabricJobSpec` stubs reserved by ADR 0007 Phase E.

Everything **between** nodes is missing: process identity, mailboxes,
links/monitors, multi-strategy supervision trees, node membership, a wire
protocol, a cluster registry, and Fabric telemetry that spans hosts.

The workflow charter ADR (`docs/platform/adrs/adr-workflow-charter.md`)
deferred clustering ("use Temporal for multi-region"). **This ADR supersedes
that deferral** for the process/distribution layer. The workflow journal
remains the durability tier; it is complementary, not competing.

## Decision

Build a BEAM/OTP-class process mesh as two new crates plus Fabric extensions.

### New crates

| Crate | Responsibility |
|-------|----------------|
| `id_effect_process` | OTP layer, purely local: typed mailboxes, `Process` behavior (`init`/`handle_call`/`handle_cast`/`handle_info`/`terminate` as Effects), `Pid`/`Addr<M>`, links, monitors, `trap_exit`, exit-signal propagation over `Cause`, supervision trees (`OneForOne`/`OneForAll`/`RestForOne`, restart intensity on `Clock`, shutdown protocol), `DynamicSupervisor`, local name registry |
| `id_effect_node` | Distribution layer: `NodeId`, cookie handshake, `Transport` trait (TCP length-delimited frames; in-memory impl for deterministic tests; QUIC as feature), SWIM-style membership with pluggable discovery, `NodeUp`/`NodeDown` events, remote pids, `BehaviorRegistry` remote spawn, remote links/monitors with `noconnection`, AP CRDT process groups, CP `ClusterSingleton` (PG advisory-lock lease), `DistributedSupervisor`, cluster telemetry aggregation feeding Fabric placement |

### Core design principles

1. **Failure is control flow.** Exit signals, links, monitors, and
   supervision trees are the primary API; `Cause<E>`
   (Fail/Die/Interrupt/Both/Then) carries them.
2. **No code mobility — registered behaviors.** Rust cannot serialize
   closures. Remote spawn names a behavior registered in the binary plus
   Schema-validated init arguments. Every node runs the same image (the
   FLAME assumption).
3. **No preemption — admission.** Fairness comes from Fabric admission
   budgets, interpreter yield points, and per-mailbox processing quotas.
4. **Location transparency with explicit types.** `Pid` is
   location-transparent for `send`; the API distinguishes local and remote
   where semantics differ.
5. **At-most-once messaging, explicitly.** Durability escalates to
   `id_effect_jobs` (outbox/Apalis), never promised by the process layer.
6. **AP by default, CP by choice.** Process-group registry is eventually
   consistent (merge-on-heal); cluster singletons opt into Postgres
   advisory-lock leases.
7. **Each library plays its proficiency.** Tokio: sockets/timers/blocking.
   Axum: control plane. Schema: wire-boundary validation. Fabric: placement.
   id_effect core: semantics.

### Process model

- `Pid = { node: NodeId, local: u64, incarnation: u32 }`. Incarnation
  prevents stale-pid aliasing after restarts.
- Each process is a supervised fiber draining a typed mailbox
  (`coordination::Queue`) with a separate **system channel** for exit
  signals, monitor downs, and stop requests — system messages are never
  blocked behind user messages.
- The process `Scope` owns all its resources; finalizers replace per-process
  GC. Process death always closes the scope.
- `call` = send request + one-shot `Deferred` reply + implicit monitor +
  timeout on `Clock` (BEAM's `GenServer.call` convention).
- **Links** are bidirectional fate-sharing: a non-`normal` exit interrupts
  linked processes unless they set `trap_exit`, in which case the exit
  arrives as `handle_info(Info::Exit { from, cause })`.
- **Monitors** are one-shot, unidirectional: exactly one
  `Down { pid, reason }` delivery (crash, normal, or `noconnection`).

### Supervision trees

```text
ChildSpec { id, start, restart: Permanent | Transient | Temporary,
            shutdown: BrutalKill | Timeout(Duration) | Infinity,
            kind: Worker | Supervisor }
Strategy   { OneForOne | OneForAll | RestForOne }
Intensity  { max_restarts, within }   // measured on the injected Clock
```

- Shutdown protocol: cancel token → wait `shutdown` timeout on `Clock` →
  hard `interrupt()`.
- Intensity exceeded ⇒ the supervisor itself fails, escalating up the tree
  (`Cause::Then` aggregation).
- `concurrency::Supervisor`'s restart-with-`Schedule` remains the leaf
  policy inside a `ChildSpec`; this ADR extends, not replaces, existing
  supervision.
- `DynamicSupervisor` covers homogeneous, template-spawned children.

### Wire protocol v1

- Framing: 4-byte big-endian length prefix, then a `postcard`-encoded
  `Envelope`.
- `Envelope { version: u16, kind, correlation: Option<u64>,
  trace_context: Option<String>, payload: Vec<u8> }`.
- Kinds: `Hello`, `Challenge`, `ChallengeReply`, `Welcome`, `Ping`, `Pong`,
  `Send`, `SysSend`, `SpawnRequest`, `SpawnReply`, `MonitorRequest`,
  `Down`, `LinkRequest`, `Exit`, `GroupDelta`, `LoadReport`, `Bye`.
- Handshake: `Hello(node, version, capabilities)` →
  `Challenge(nonce)` → `ChallengeReply(mac(cookie, nonce))` → `Welcome`.
  Version negotiation is mandatory; mismatched majors refuse to connect.
- Payloads cross the trust boundary as bytes and are decoded through
  `id_effect::schema` (`Unknown` → typed decode with `ParseErrors`):
  parse, don't validate.
- OTel context (`trace_context`) propagates in the envelope.

### Membership

- SWIM-style: periodic probe → suspect → confirm-down, piggybacking
  membership deltas and `LoadReport` telemetry on probe traffic.
- Discovery strategies: static peer list, DNS (Kubernetes headless
  service), gossip seed.
- No full-mesh requirement: membership is gossip; data connections are
  direct, on demand.
- All timing runs on the injected `Clock`, so the in-memory transport +
  `TestClock` make partition scenarios fully deterministic.

### Guarantees (normative)

| Primitive | Guarantee |
|-----------|-----------|
| Local send | Ordered per sender→receiver pair; never lost while receiver alive |
| Remote send | At-most-once; ordered per pair per connection epoch; dropped on disconnect |
| Monitor | Exactly one `Down` (crash, normal, or `noconnection`) |
| Link | Exit propagation or trapped message; `noconnection` on node loss |
| Group registry (AP) | Eventually consistent; duplicate/gap windows during partition; merge on heal |
| ClusterSingleton (CP) | At most one holder cluster-wide; unavailable without PG |
| Durable work | At-least-once with idempotency keys — `id_effect_jobs` only |

### Netsplit stories (normative)

- **Send:** in-flight messages to a partitioned node are dropped; senders
  needing certainty must monitor.
- **Monitors/links:** partition ⇒ `Down`/`Exit` with reason
  `noconnection` after suspicion timeout. On heal, no resurrection: stale
  pids from the previous connection epoch are dead.
- **Groups:** each side keeps operating on its view; ORSWOT merge on heal
  (add-wins).
- **DistributedSupervisor:** on `NodeDown`, re-places children on surviving
  nodes. Under partition, both sides may run a copy (AP); singletons that
  must not duplicate use `ClusterSingleton` (CP) instead.
- **ClusterSingleton:** the lease holder's side of a partition keeps the
  singleton while its PG session lives; the other side cannot acquire.

### Fabric integration

- `TelemetrySnapshot`s from every node piggyback on membership traffic;
  `compute::cluster` gains a `PlacementScorer` that ranks candidate nodes
  by load, `WorkProfile` affinity, and `PlacementMode`
  (LocalFirst / Spread / Affinity).
- `RebalanceStrategy::ScaleOut` becomes real: saturation triggers remote
  spawns through `id_effect_node`, or durable `id_effect_jobs` enqueue when
  work is marked durable.
- Stream fan-out across nodes is expressed as ordinary stream combinators
  over process groups with admission-bounded concurrency per node.

## Non-goals (v1)

- Hot code loading / rolling cluster upgrades across incompatible majors.
- Cross-version clusters (protocol major must match).
- Exactly-once messaging (use jobs/outbox).
- Preemptive scheduling of native code.
- Byzantine fault tolerance; the cookie boundary assumes a trusted cluster.
- Replacing the workflow journal or Temporal for long-lived durable
  orchestration.

## Consequences

- Two new workspace crates; core `compute/cluster.rs` stubs become the real
  placement API; `FabricJobRunner` in `id_effect_jobs` gets an
  implementation.
- The book gains Part VII (Process Mesh).
- Deterministic distributed testing (in-memory transport + `TestClock`)
  becomes the acceptance bar — a capability BEAM itself lacks.
- Supersedes the "defer clustering to Temporal" stance of the workflow
  charter ADR for the process/distribution layer.

## Phased delivery

| Phase | Scope |
|-------|-------|
| 0 | This ADR, mission, semantics tables |
| 1 | `id_effect_process`: mailboxes, behaviors, links/monitors/trap_exit, supervision trees, DynamicSupervisor, registry |
| 2 | `id_effect_node`: transport (TCP/in-memory; QUIC feature), handshake, SWIM membership, discovery, node monitors |
| 3 | Remote processes: envelope + Schema boundary, BehaviorRegistry, remote spawn/send/call/links/monitors, groups (AP), singleton (CP), OTel propagation |
| 4 | DistributedSupervisor takeover; cluster telemetry aggregation; placement scoring; real ScaleOut; cluster stream fan-out; cross-node backpressure |
| 5 | Axum control plane; chaos suite; soak benchmarks; Book Part VII; examples 130–133 |
