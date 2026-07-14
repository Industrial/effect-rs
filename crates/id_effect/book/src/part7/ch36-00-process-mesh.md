# Part VII: Process Mesh

The Process Mesh brings BEAM/OTP-class processes, supervision trees, and
multi-node distribution to `id_effect` (ADR 0009).

## Crates

- **`id_effect_process`** — local OTP: `Process`, mailboxes, links/monitors,
  `trap_exit`, multi-strategy supervisors, DynamicSupervisor, local registry.
- **`id_effect_node`** — distribution: NodeId, cookie handshake, transports
  (InMemory/TCP/QUIC), SWIM membership (`foca`), remote spawn/send/call,
  process groups (ORSWOT), ClusterSingleton, DistributedSupervisor.

## Guarantees (summary)

| Primitive | Guarantee |
|-----------|-----------|
| Local send | Ordered per pair; never lost while alive |
| Remote send | At-most-once; ordered per connection epoch |
| Monitors | Exactly one `Down` |
| Process groups | Eventually consistent |
| Singleton | At most one holder (CP via lease) |

See [ADR 0009](../../../docs/adrs/0009-process-mesh.md) for netsplit stories.
