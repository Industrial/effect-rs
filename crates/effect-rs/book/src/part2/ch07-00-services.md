# Services — The Complete DI Pattern

The previous chapters established the building blocks: Tags (identities), Context (the environment), and Layers (constructors). Now we put them together into the complete *Service* pattern.

A Service in effect-rs is the combination of:
1. A **trait** defining the interface
2. A **tag** identifying it in the environment
3. One or more **implementations** (production and test)
4. A **layer** that wires an implementation into the environment

This is the full dependency injection story. By the end of this chapter you'll have a working multi-service application wired entirely at compile time.
