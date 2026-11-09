#!/usr/bin/env bash
# Release build of Linux + Windows binaries.
# Same as: ./scripts/build.sh --release

set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
exec "$ROOT/scripts/build.sh" --release "$@"
