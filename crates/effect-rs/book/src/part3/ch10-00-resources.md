# Resources & Scopes — Deterministic Cleanup

RAII works beautifully in synchronous Rust: resources are released when they fall out of scope, `Drop` runs deterministically. In async code, the picture gets complicated.

This chapter shows why RAII breaks down in async contexts, introduces `Scope` and finalizers as the solution, covers the `acquire_release` pattern for RAII-style resource management, and concludes with `Pool` for reusing expensive connections.
