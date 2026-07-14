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
deterministic restart-storm tests.

**Runnable example:** `id_effect_process/examples/130_process_tree.rs`.
