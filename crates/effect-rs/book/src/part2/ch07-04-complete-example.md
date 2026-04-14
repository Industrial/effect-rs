# A Complete DI Example — Putting It All Together

This section builds a small but complete application with three services, a Layer graph, and both production and test wiring. It's the capstone of Part II.

## The Domain

A blog API with users, posts, and notifications:

```rust
// Domain types
struct User { id: u64, name: String, email: String }
struct Post { id: u64, author_id: u64, title: String, body: String }
enum AppError { Db(DbError), Notify(NotifyError) }
```

## Three Service Traits

```rust
trait UserRepository: Send + Sync {
    fn get_user(&self, id: u64) -> Effect<User, DbError, ()>;
}

trait PostRepository: Send + Sync {
    fn get_posts_by_author(&self, author_id: u64) -> Effect<Vec<Post>, DbError, ()>;
}

trait NotificationService: Send + Sync {
    fn send_welcome(&self, to: &str) -> Effect<(), NotifyError, ()>;
}

service_key!(UserRepoKey:   Arc<dyn UserRepository>);
service_key!(PostRepoKey:   Arc<dyn PostRepository>);
service_key!(NotifierKey:   Arc<dyn NotificationService>);
```

## The Business Logic

```rust
fn get_author_feed(author_id: u64)
-> Effect<(User, Vec<Post>), AppError, impl NeedsUserRepo + NeedsPostRepo>
{
    effect! {
        let user_repo = ~ UserRepoKey;
        let post_repo = ~ PostRepoKey;
        let user  = ~ user_repo.get_user(author_id).map_error(AppError::Db);
        let posts = ~ post_repo.get_posts_by_author(author_id).map_error(AppError::Db);
        (user, posts)
    }
}

fn register_user(name: &str, email: &str)
-> Effect<User, AppError, impl NeedsUserRepo + NeedsNotifier>
{
    effect! {
        let repo     = ~ UserRepoKey;
        let notifier = ~ NotifierKey;
        let user = ~ repo.create_user(name, email).map_error(AppError::Db);
        ~ notifier.send_welcome(&user.email).map_error(AppError::Notify);
        user
    }
}
```

## Production Layer Graph

```rust
let prod_graph = LayerGraph::new()
    .add(LayerNode::new("config",   config_layer))
    .add(LayerNode::new("db",       pg_db_layer).requires("config"))
    .add(LayerNode::new("users",    pg_user_repo_layer).requires("db"))
    .add(LayerNode::new("posts",    pg_post_repo_layer).requires("db"))
    .add(LayerNode::new("notifier", smtp_notifier_layer).requires("config"));

fn main() {
    let plan = prod_graph.plan().expect("layer graph has no cycles");
    run_blocking(
        get_author_feed(1).provide_layer(plan)
    ).expect("app failed");
}
```

## Test Wiring

```rust
fn make_test_layer() -> impl Layer<Output, (), Nil> {
    let users = mock_user_repo(&[alice(), bob()]);
    let posts  = mock_post_repo(&[alice_post()]);
    let notify = capturing_notifier();

    user_repo_layer(users)
        .stack(post_repo_layer(posts))
        .stack(notifier_layer(notify))
}

#[test]
fn feed_includes_authors_posts() {
    let result = run_test(
        get_author_feed(1).provide_layer(make_test_layer())
    ).unwrap();
    assert_eq!(result.1.len(), 1);
    assert_eq!(result.1[0].title, "Alice's Post");
}

#[test]
fn registration_sends_welcome_email() {
    let notifier = capturing_notifier();
    let result = run_test(
        register_user("Carol", "carol@example.com")
            .provide_layer(
                mock_user_repo_layer(&[]).stack(notifier_layer(notifier.clone()))
            )
    );
    assert!(result.is_ok());
    assert_eq!(notifier.sent_count(), 1);
}
```

## What This Demonstrates

The business logic functions (`get_author_feed`, `register_user`) are completely decoupled from the infrastructure:
- They declare their needs via `NeedsX` bounds
- They access services via `~ ServiceKey`
- They know nothing about Postgres, SMTP, or any concrete type

The Layer graph wires concrete implementations at the entry point. Swapping Postgres for SQLite or SMTP for SendGrid requires changing only the layer definition — not a single line of business logic.

That's compile-time dependency injection: the full dependency graph is verified by the type checker, not discovered at runtime.

---

You've completed Part II. The next section of the book shifts to operational concerns: how to handle errors properly, run fibers concurrently, manage resources safely, and schedule repeated work.
