# Clusters: groups, singletons, distributed supervision

**Process groups** are an ORSWOT CRDT (`ProcessGroups`) — AP by default;
duplicate/gap windows under partition, merge on heal.

**ClusterSingleton** is CP: at most one holder via a lease backend. `MemoryLease`
is the in-process test backend; [`PgAdvisoryLease`](../../id_effect_node/src/singleton.rs)
(crate feature `pg`) is the production path, holding a Postgres session-scoped
advisory lock on a dedicated worker connection so the lock releases with the session.

**DistributedSupervisor** re-places children on `NodeDown` (Horde-style
takeover) using the behavior registry + mesh placement. It honors each child's
`prefer` hint and excludes downed nodes when choosing a takeover target.

Fabric placement routes every decision through the single [`place`](../../src/compute/cluster.rs)
scorer, fed by [`LoadReport`](../../src/compute/cluster.rs) telemetry and bounded by
`ClusterResourcePolicy`. `scale_out_placed` records the chosen `target_node` so the
job runner dispatches work to the right node.

**Runnable examples:**
`id_effect_node/examples/132_distributed_supervisor_failover.rs` (takeover on
`NodeDown`) and `id_effect_node/examples/133_cluster_stream_fanout.rs`
(`fanout_cast` to a process group across nodes).
