# Processes and supervision trees

A `Process` is a GenServer-shaped behavior: `init`, `handle_call`,
`handle_cast`, `handle_info`, `terminate` — each returns `Effect`.

```rust,ignore
use id_effect_process::{Process, ProcessNode, Next, NodeName};

let node = ProcessNode::threads(NodeName::new("app@local"));
let addr = node.spawn::<MyProc>(args, env);
```

Supervision:

```rust,ignore
use id_effect_process::{SupervisorSpec, Strategy, ChildSpec};

let handle = SupervisorSpec::new(Strategy::OneForOne)
  .intensity(3, Duration::from_secs(5))
  .child(ChildSpec::worker::<Worker, _>("w", || ((), ())))
  .start(&node);
```

Restart intensity is measured on the injected `Clock` — use `TestClock` for
deterministic restart-storm tests. The sliding-window accounting is the shared
[`concurrency::RestartIntensity`](../../src/concurrency/supervisor.rs) primitive
(a max number of restarts within a rolling period), distinct from
`RestartWithLimit`'s total-count cap. The OTP supervisor delegates to it so the
local and clustered supervisors share one intensity definition.

**Runnable example:** `id_effect_process/examples/130_process_tree.rs`.
