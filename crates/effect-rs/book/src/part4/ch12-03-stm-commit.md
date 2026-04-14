# Stm and commit — Building Transactions

The `stm!` macro produces `Stm<A>` values — descriptions of transactional computations. To execute them, you use `commit` or `atomically`.

## commit: Lift Stm into Effect

```rust
use effect::{commit, Stm, Effect};

let transaction: Stm<i32> = stm! {
    let a = ~ ref_a.read_stm();
    let b = ~ ref_b.read_stm();
    a + b
};

// Lift into Effect
let effect: Effect<i32, Never, ()> = commit(transaction);

// Now run it
let result = run_blocking(effect)?;
```

`commit` wraps a `Stm` in an effect that, when run, executes the transaction and retries if there's a conflict. The `E` type of `commit(stm)` is `Never` unless the `Stm` can fail (see `stm::fail`).

## atomically: Direct Execution

```rust
use effect::atomically;

// Run a transaction immediately in the current context
let value: i32 = atomically(stm! {
    ~ counter.modify_stm(|n| n + 1);
    ~ counter.read_stm()
});
```

`atomically` is the synchronous equivalent of `commit` + `run_blocking`. Use it when you're already outside the effect system and need a quick transactional update.

## stm::fail: Transactional Errors

Transactions can fail with typed errors:

```rust
use effect::stm;

fn withdraw(account: &TRef<u64>, amount: u64) -> Stm<u64> {
    stm! {
        let balance = ~ account.read_stm();
        if balance < amount {
            ~ stm::fail(InsufficientFunds);  // abort the transaction
        }
        ~ account.write_stm(balance - amount);
        balance - amount
    }
}

// commit propagates the error into E
let effect: Effect<u64, InsufficientFunds, ()> = commit(withdraw(&account, 100));
```

`stm::fail(e)` aborts the current transaction with error `e`. The transaction is *not* retried — it fails immediately with the given error.

## stm::retry: Block Until Condition

Sometimes a transaction should wait until a condition is true rather than failing:

```rust
// Block (retry) until the queue has items
fn dequeue(queue: &TRef<Vec<Item>>) -> Stm<Item> {
    stm! {
        let items = ~ queue.read_stm();
        if items.is_empty() {
            ~ stm::retry();  // block until queue changes, then retry
        }
        let item = items[0].clone();
        ~ queue.write_stm(items[1..].to_vec());
        item
    }
}
```

`stm::retry()` doesn't mean "try again immediately." It means "block until any `TRef` I read has changed, then try again." This is how `TQueue` implements blocking dequeue without busy-waiting.

## Composing Transactions

Transactions compose by sequencing `stm!` blocks:

```rust
let big_transaction: Stm<()> = stm! {
    // Sub-transaction 1
    let _ = ~ transfer_funds(&from, &to, amount);
    // Sub-transaction 2
    let _ = ~ record_audit_log(&from, &to, amount);
    ()
};

// Both operations commit atomically or neither does
let effect = commit(big_transaction);
```

The composed transaction retries as a unit — if either sub-operation sees a conflict, the whole thing restarts from the beginning.
