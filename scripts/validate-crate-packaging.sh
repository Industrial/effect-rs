#!/usr/bin/env bash
# Validate that workspace crates can be packaged for crates.io publish.
#
# Checks:
# 1) Publish-order: path+version pins must not require crates published later
#    (cross-level or later in the same publish.yml wave).
# 2) Dev-deps: workspace path deps in [dev-dependencies] must be path-only.
#    cargo publish still resolves version against crates.io for path+version.
# 3) cargo package with temporary [patch.crates-io] (sibling crates resolve locally).
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

skip=id_effect_lint

# Publish levels from .github/workflows/publish.yml (lower = earlier).
declare -A LEVEL=(
    [id_effect_graph]=0
    [id_effect_macro]=0
    [id_effect_proc_macro]=0
    [id_effect]=1
    [id_effect_process]=2
    [id_effect_cli]=2
    [id_effect_jobs]=2
    [id_effect_logger]=2
    [id_effect_optics]=2
    [id_effect_parse]=2
    [id_effect_platform]=2
    [id_effect_resilience]=2
    [id_effect_sql]=2
    [id_effect_tokio]=2
    [id_effect_config]=3
    [id_effect_sql_pg]=3
    [id_effect_tower]=3
    [id_effect_ai]=4
    [id_effect_node]=4
    [id_effect_axum]=4
    [id_effect_events]=4
    [id_effect_opentelemetry]=4
    [id_effect_workflow]=4
    [id_effect_fsm]=5
    [id_effect_rpc]=5
)

# Same-wave publish order (must match .github/workflows/publish.yml step order).
declare -a ORDER=(
    id_effect_graph id_effect_macro id_effect_proc_macro
    id_effect
    id_effect_cli id_effect_jobs id_effect_logger id_effect_optics
    id_effect_parse id_effect_platform id_effect_resilience id_effect_sql id_effect_tokio id_effect_process
    id_effect_config id_effect_sql_pg id_effect_tower
    id_effect_ai id_effect_node id_effect_axum id_effect_events id_effect_opentelemetry id_effect_workflow
    id_effect_fsm id_effect_rpc
)
declare -A ORD_IDX=()
for i in "${!ORDER[@]}"; do ORD_IDX["${ORDER[$i]}"]=$i; done

failed=0

in_dev_deps_section() {
    local toml="$1" needle="$2"
    awk -v needle="$needle" '
    /^\[[^]]*\]/ { sec=$0 }
    index($0, needle) {
      print (sec ~ /^\[dev-dependencies\]/) ? "yes" : "no"
      exit
    }
  ' "$toml"
}

# --- Check 1+2: publish-order + path-only dev-deps ---
while IFS= read -r toml; do
    crate=$(grep -m1 '^name = ' "$toml" | sed 's/^name = "\(.*\)"/\1/')
    [[ "$crate" == "$skip" ]] && continue
    from_level=${LEVEL[$crate]:-}
    [[ -z "$from_level" ]] && continue
    from_ord=${ORD_IDX[$crate]:-}

    while IFS= read -r line; do
        dep=$(echo "$line" | sed -n 's/^\([a-zA-Z0-9_]*\) *=.*path *=.*version.*/\1/p')
        [[ -z "$dep" ]] && continue
        to_level=${LEVEL[$dep]:-}
        [[ -z "$to_level" ]] && continue

        if [[ "$(in_dev_deps_section "$toml" "$line")" == "yes" ]]; then
            echo "ERROR: $crate [dev-dependencies] pins $dep with path+version"
            echo "  → cargo publish resolves the version on crates.io even for path deps."
            echo "  → Use path-only: $dep = { path = \"../$dep\" }"
            echo "  in $toml: $line"
            failed=1
            continue
        fi

        if (( to_level > from_level )); then
            echo "ERROR: $crate (publish level $from_level) pins $dep version but $dep publishes at level $to_level"
            echo "  → cargo publish will look for $dep on crates.io before it exists."
            echo "  → For test-only deps use path without version; for runtime deps fix publish order."
            echo "  in $toml: $line"
            failed=1
            continue
        fi

        to_ord=${ORD_IDX[$dep]:-}
        if [[ -n "$from_ord" && -n "$to_ord" ]] && (( to_level == from_level && to_ord > from_ord )); then
            echo "ERROR: $crate pins $dep version but $dep publishes later in the same level ($from_level)"
            echo "  → Reorder publish.yml or use path-only if this is a test-only dependency."
            echo "  in $toml: $line"
            failed=1
        fi
    done < <(grep -E '^[a-zA-Z0-9_]+ = \{.*path = .*version =' "$toml" || true)
done < <(find crates -mindepth 2 -maxdepth 2 -name Cargo.toml | sort)

# --- Check 3: cargo package with local patch ---
cargo_dir=$(mktemp -d)
trap 'rm -rf "$cargo_dir"' EXIT
export CARGO_HOME="$cargo_dir"

mkdir -p "$CARGO_HOME"
{
    echo '[patch.crates-io]'
    while IFS= read -r toml; do
        crate=$(grep -m1 '^name = ' "$toml" | sed 's/^name = "\(.*\)"/\1/')
        [[ "$crate" == "$skip" ]] && continue
        dir=$(cd "$(dirname "$toml")" && pwd)
        echo "$crate = { path = \"$dir\" }"
    done < <(find crates -mindepth 2 -maxdepth 2 -name Cargo.toml | sort)
} > "$CARGO_HOME/config.toml"

while IFS= read -r toml; do
    crate=$(grep -m1 '^name = ' "$toml" | sed 's/^name = "\(.*\)"/\1/')
    [[ "$crate" == "$skip" ]] && continue

    if readme=$(grep -m1 '^readme = ' "$toml" | sed 's/^readme = "\(.*\)"/\1/'); then
        dir=$(dirname "$toml")
        if [[ ! -f "$dir/$readme" ]]; then
            echo "ERROR: $crate declares readme = \"$readme\" but $dir/$readme is missing"
            failed=1
            continue
        fi
    fi

    if ! cargo package --package "$crate" --no-verify --allow-dirty --quiet 2>/tmp/validate-pkg.err; then
        echo "ERROR: cargo package failed for $crate"
        sed 's/^/  /' /tmp/validate-pkg.err
        failed=1
    fi
done < <(find crates -mindepth 2 -maxdepth 2 -name Cargo.toml | sort)

if [[ "$failed" -ne 0 ]]; then
    exit 1
fi

echo "All publishable crates pass packaging validation"
