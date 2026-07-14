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

On `NodeDown`, remote monitors receive `ExitReason::NoConnection`.
