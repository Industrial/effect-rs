# Why STM? — The Shared State Problem

Consider transferring money between two accounts. With mutexes:

```rust
fn transfer(from: &Mutex<Account>, to: &Mutex<Account>, amount: u64) {
    let from_guard = from.lock().unwrap();
    // Thread B might be doing transfer(to, from, ...) right here
    let to_guard = to.lock().unwrap();
    // DEADLOCK: Thread A holds from, waiting for to.
    //           Thread B holds to, waiting for from.
    from_guard.balance -= amount;
    to_guard.balance += amount;
}
```

The standard fix (always lock in a consistent order) requires global coordination across your codebase. Add a third account and you need to sort three locks. It doesn't compose.

## STM: Optimistic Concurrency

STM operates on the assumption that conflicts are rare. Instead of locking, it:

1. Reads current values into a local transaction log
2. Computes new values based on those reads
3. Attempts to commit: checks that nothing changed since the reads, then atomically writes

If anything changed between step 1 and step 3, the transaction *retries* automatically from step 1.

```rust
use effect::{TRef, stm, commit};

fn transfer(from: &TRef<Account>, to: &TRef<Account>, amount: u64)
-> Effect<(), TransferError, ()>
{
    commit(stm! {
        let from_acct = ~ from.read_stm();
        let to_acct   = ~ to.read_stm();

        if from_acct.balance < amount {
            ~ stm::fail(TransferError::InsufficientFunds);
        }

        ~ from.write_stm(Account { balance: from_acct.balance - amount, ..from_acct });
        ~ to.write_stm(Account { balance: to_acct.balance + amount, ..to_acct });
        ()
    })
}
```

No locks. No deadlock risk. The transaction retries automatically if another transaction modified either account between our read and our write.

## When STM Wins

| Situation | Mutex | STM |
|-----------|-------|-----|
| Single shared value | ✓ simple | ✓ fine |
| Multiple related values | ✗ deadlock risk | ✓ composable |
| Read-heavy workloads | ✗ blocks writers | ✓ reads never block |
| Composing two existing operations | ✗ requires coordination | ✓ just nest in stm! |
| Long operations with I/O | ✓ (STM would retry too much) | ✗ wrong tool |

STM shines when:
- You need to update multiple values atomically
- You're composing smaller transactional operations into larger ones
- Contention is low (retries are cheap)

Avoid STM for long-running operations that do I/O — transactions should be short and pure. The `stm!` macro is for read-modify-write, not for network calls.
