# TQueue, TMap, TSemaphore — Transactional Collections

effect-rs provides STM-aware versions of common collection types. They compose with other STM operations and integrate with `stm!`.

## TQueue: Bounded Transactional Queue

```rust
use effect::TQueue;

let queue: TQueue<Job> = TQueue::bounded(100);

// Enqueue (blocks/retries if full)
let offer: Stm<()> = queue.offer_stm(job);

// Dequeue (blocks/retries if empty)
let take: Stm<Job> = queue.take_stm();

// Peek without removing
let peek: Stm<Option<Job>> = queue.peek_stm();

// Non-blocking try
let try_take: Stm<Option<Job>> = queue.try_take_stm();
```

`TQueue::bounded(n)` creates a queue with capacity `n`. `offer_stm` blocks (via `stm::retry`) when full; `take_stm` blocks when empty. Both integrate naturally with `stm!`.

### Producer-Consumer Pattern

```rust
fn producer(queue: Arc<TQueue<Job>>, jobs: Vec<Job>) -> Effect<(), Never, ()> {
    effect! {
        for job in jobs {
            ~ commit(queue.offer_stm(job));
        }
        Ok(())
    }
}

fn consumer(queue: Arc<TQueue<Job>>) -> Effect<Never, Never, ()> {
    effect! {
        loop {
            let job = ~ commit(queue.take_stm());  // blocks if empty
            ~ process_job(job);
        }
    }
}
```

## TMap: Transactional Hash Map

```rust
use effect::TMap;

let map: TMap<String, User> = TMap::new();

// Inside stm!:
let insert: Stm<()> = map.insert_stm("alice".into(), alice_user);
let get:    Stm<Option<User>> = map.get_stm("alice");
let remove: Stm<Option<User>> = map.remove_stm("alice");
let update: Stm<()> = map.modify_stm("alice", |u| { u.name = "ALICE".into(); u });
```

`TMap` is a concurrent hash map where all operations participate in STM transactions. Reading from `TMap` and `TRef` in the same transaction is atomic:

```rust
commit(stm! {
    let user = ~ user_map.get_stm("alice");
    let count = ~ access_counter.read_stm();
    ~ access_counter.write_stm(count + 1);
    user
})
// Either the map read AND the counter increment happen, or neither does
```

## TSemaphore: Transactional Semaphore

```rust
use effect::TSemaphore;

// Create a semaphore with 10 permits
let sem: TSemaphore = TSemaphore::new(10);

// Acquire 1 permit (blocks if none available)
let acquire: Stm<()> = sem.acquire_stm(1);

// Release 1 permit
let release: Stm<()> = sem.release_stm(1);
```

`TSemaphore` limits concurrent access to a resource. Use it with `acquire_release` for resource pools where you want transactional semantics:

```rust
commit(stm! {
    ~ sem.acquire_stm(1);  // blocks until permit available
    ()
}).flat_map(|()| {
    do_limited_work().flat_map(|result| {
        commit(sem.release_stm(1)).map(|()| result)
    })
})
```

## Summary

| Type | Purpose |
|------|---------|
| `TRef<T>` | Single mutable value |
| `TQueue<T>` | Blocking FIFO queue |
| `TMap<K, V>` | Concurrent hash map |
| `TSemaphore` | Concurrency limiter |

All compose inside `stm!` and commit atomically with other STM operations.
