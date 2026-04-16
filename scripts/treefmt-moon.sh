#!/usr/bin/env bash
# Moon tasks call this instead of `treefmt` directly so `moon run :check` works when
# treefmt is only provided by the dev shell (devenv.nix). CI installs treefmt on PATH.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/.." && pwd)"
cd "$root"
if command -v treefmt >/dev/null 2>&1; then
    exec treefmt "$@"
fi
if command -v devenv >/dev/null 2>&1; then
    exec devenv shell -- treefmt "$@"
fi
echo "treefmt: command not found. Enter the dev shell (treefmt is in devenv.nix), e.g.:" >&2
echo "  devenv shell -- moon run :check" >&2
exit 127
