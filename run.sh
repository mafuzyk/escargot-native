#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}"
exec cargo run -j 4 --bin escargot-native -- "$@"
