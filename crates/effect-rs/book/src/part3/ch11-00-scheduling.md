# Scheduling — Retry, Repeat, and Time

Production services fail. Networks are unreliable. Downstream APIs go down. The database gets overwhelmed. Defensive engineering means anticipating failure and building policies for what to do when it happens.

effect-rs models these policies with `Schedule` — a type that describes when to retry, how long to wait between attempts, and when to give up. Combined with `Clock` injection, scheduling logic becomes testable without real-time delays.
