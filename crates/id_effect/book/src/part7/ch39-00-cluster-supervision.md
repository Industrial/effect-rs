# Clusters: groups, singletons, distributed supervision

**Process groups** are an ORSWOT CRDT (`ProcessGroups`) — AP by default;
duplicate/gap windows under partition, merge on heal.

**ClusterSingleton** is CP: at most one holder via a lease backend
(`MemoryLease` for tests; PG advisory locks in production).

**DistributedSupervisor** re-places children on `NodeDown` (Horde-style
takeover) using the behavior registry + mesh placement.

Fabric placement (`pick_placement` / `scale_out_placed`) scores candidate
nodes by telemetry headroom under `ClusterResourcePolicy`.
