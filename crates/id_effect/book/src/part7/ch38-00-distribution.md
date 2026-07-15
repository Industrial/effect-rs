# Nodes, membership, and remote processes

Every node has a `NodeId` (name + boot incarnation). Peers authenticate with
the cookie HMAC handshake over a `Transport` (QUIC preferred, TCP fallback,
`InMemoryTransport` for tests).

Membership is SWIM via `foca` (sans-io). Drive packets and timers yourself
for deterministic partition tests, or pump them over Tokio in production.

Remote spawn names a **registered behavior** plus postcard args (no code
mobility):

```rust,ignore
registry.register::<Counter>("counter");
let pid = mesh.remote_spawn("b@host", "counter", args, None, timeout)?;
mesh.remote_cast(&pid, postcard::to_allocvec(&msg)?)?;
```

For load-aware placement, nodes publish telemetry with `mesh.report_load(..)`
(readable via `mesh.load_reports()`), and `mesh.remote_spawn_placed(..)` picks a
target through the unified [`place`](../../src/compute/cluster.rs) scorer instead
of naming a node directly.

On `NodeDown`, remote monitors receive `ExitReason::NoConnection`.

**Runnable example:** `id_effect_node/examples/131_two_node_cluster.rs`.
