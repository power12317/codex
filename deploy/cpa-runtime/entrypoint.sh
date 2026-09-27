#!/bin/sh
set -eu
export CODEX_CPA_AUTH_DIR="${CODEX_CPA_AUTH_DIR:-/cpa-auth}"
exec /usr/local/bin/codex-app-server "$@"
