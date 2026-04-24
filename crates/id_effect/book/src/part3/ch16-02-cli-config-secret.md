# Config + `Secret` from flags

[`id_effect_config`](../part2/ch07-10-config.md) already documents providers and [`Config`](https://docs.rs/id_effect_config/latest/id_effect_config/struct.Config.html) descriptors. At a **CLI edge**, you usually:

1. **Parse** a `--token` / `--api-key` flag (or read a path to a file).
2. **Seed** a [`MapConfigProvider`](https://docs.rs/id_effect_config/latest/id_effect_config/struct.MapConfigProvider.html) (or `EnvConfigProvider`) so the key matches what your `Config::*` descriptors expect.
3. **Evaluate** `Config::…` inside an `Effect` using [`config_env`](https://docs.rs/id_effect_config/latest/id_effect_config/fn.config_env.html) in `R`.
4. **Wrap** sensitive strings with [`Secret`](https://docs.rs/id_effect_config/latest/id_effect_config/struct.Secret.html) via [`Config::secret`](https://docs.rs/id_effect_config/latest/id_effect_config/struct.Config.html#method.secret) so logs and `Debug` never print raw material.

## Minimal snippet

```rust
use id_effect::Effect;
use id_effect_config::{
    Config, ConfigError, MapConfigProvider, Secret, config_env,
};

fn load_api_token() -> Effect<Secret<String>, ConfigError, id_effect_config::ConfigEnv> {
    Config::string("API_TOKEN").secret().run::<Secret<String>, ConfigError, _>()
}

// In main: build provider from CLI flag, then `config_env(provider)` and `run_blocking`.
```

The repository ships a full runnable layout under **[`examples/cli-minimal`](https://github.com/Industrial/id_effect/tree/main/examples/cli-minimal)** (`cli_minimal` package in the workspace).

## Operational notes

- Prefer **short-lived** exposure of secrets: parse → wrap in [`Secret`](https://docs.rs/id_effect_config/latest/id_effect_config/struct.Secret.html) → pass to effects; avoid storing raw `String` copies in globals.
- For file-based secrets, read bytes in an effect and wrap immediately; redact paths in error messages if they reveal usernames.
- Combine with [CLI with clap](./ch16-00-cli-with-clap.md) for argv parsing and [exit codes](./ch16-01-cli-exit-codes.md) for `main`.
