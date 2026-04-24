# Minimal CLI example (Phase E)

Workspace package [`cli_minimal`](./Cargo.toml) mirrors the mdBook **CLI with clap** chapter:

- Parse flags with [`clap`](https://docs.rs/clap).
- Load [`Secret`](https://docs.rs/id_effect_config/latest/id_effect_config/struct.Secret.html) configuration via [`id_effect_config`](https://docs.rs/id_effect_config).
- Finish with [`id_effect_cli::run_main`](https://docs.rs/id_effect_cli).

## Build / run (from repo root)

```bash
devenv shell -- cargo build -p cli_minimal
devenv shell -- cargo run -p cli_minimal -- --token demo
```

Or with Moon:

```bash
devenv shell -- moon run cli_minimal:build
```

## Tests

```bash
devenv shell -- cargo nextest run -p cli_minimal
```
