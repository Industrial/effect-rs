# Providing Services via Layers

You have a trait. You have an implementation. Now you need a Layer that wires them together.

## The Minimal Service Layer

```rust
use effect::{LayerFn, tagged, effect};

// The production implementation
struct PostgresUserRepository {
    pool: Pool,
}

impl UserRepository for PostgresUserRepository {
    fn get_user(&self, id: u64) -> Effect<User, DbError, ()> {
        effect! {
            let row = ~ query(&self.pool, "SELECT * FROM users WHERE id = $1", id);
            User::from_row(row)
        }
    }
    // ...
}

// The layer that builds the implementation from the database pool
let user_repo_layer = LayerFn::new(|env: &Tagged<DatabaseKey>| {
    effect! {
        let repo = PostgresUserRepository { pool: env.value().clone() };
        tagged::<UserRepositoryTag>(Arc::new(repo) as Arc<dyn UserRepository>)
    }
});
```

The layer takes `Tagged<DatabaseKey>` (the database pool) and produces `Tagged<UserRepositoryTag>` (the repository wrapped as `Arc<dyn UserRepository>`).

## Composition with Other Layers

The repository layer needs the database. Wire them:

```rust
let app_layer = config_layer
    .stack(db_layer)
    .stack(user_repo_layer);  // db_layer output feeds into user_repo_layer
```

Now `app_layer` produces an environment containing `ConfigKey`, `DatabaseKey`, and `UserRepositoryTag` — everything `get_user_profile` needs.

## Test Layer with Mock

```rust
struct MockUserRepository {
    users: HashMap<u64, User>,
}

impl UserRepository for MockUserRepository {
    fn get_user(&self, id: u64) -> Effect<User, DbError, ()> {
        match self.users.get(&id) {
            Some(u) => succeed(u.clone()),
            None    => fail(DbError::NotFound),
        }
    }
    // ...
}

let test_repo_layer = LayerFn::new(|_: &Nil| {
    let repo = MockUserRepository {
        users: [(1, alice()), (2, bob())].into(),
    };
    succeed(tagged::<UserRepositoryTag>(Arc::new(repo) as Arc<dyn UserRepository>))
});
```

The test layer needs nothing (no real database), produces the same `Tagged<UserRepositoryTag>`, and can be stacked in place of the production layer.

## The Swap

```rust
// Production
run_blocking(my_app().provide_layer(
    config_layer.stack(db_layer).stack(user_repo_layer)
));

// Test
run_test(my_app().provide_layer(
    test_repo_layer  // no config or db needed
));
```

The application code doesn't change. Only the layer stack changes. The type system ensures both stacks satisfy the effect's requirements.
