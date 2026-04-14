# Layers — Building Your Dependency Graph

You've seen how `R` encodes what an effect needs, and how `Context` holds the values at runtime. But who *builds* the context?

In small programs you can construct context manually with `ctx!` and hand values to `provide`. In real applications, you need something more powerful: a way to declare *how* to build each piece of the environment, with automatic dependency ordering and lifecycle management.

That's what **Layers** are for.

A Layer is a recipe for building part of an environment. It knows what it produces, what it needs to produce it, and (optionally) how to clean up afterward. Wire Layers together, and effect-rs figures out the right build order automatically.
