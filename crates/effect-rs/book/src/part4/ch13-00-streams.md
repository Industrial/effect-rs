# Streams — Backpressure and Chunked Processing

An `Effect` produces one value. A `Stream` produces many values over time. When you need to process a potentially infinite or very large sequence — database result sets, event logs, file lines, sensor readings — `Stream` is the right abstraction.

This chapter covers when to use `Stream` vs `Effect`, how streams process data in `Chunk`s for efficiency, how to control flow with backpressure policies, and how to consume streams with `Sink`.
