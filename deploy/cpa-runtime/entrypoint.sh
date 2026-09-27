#!/bin/sh
set -eu
: "${CODEX_CPA_AUTH_FILE:?Set the shared CPA credential file}"
: "${CODEX_CPA_WORKER_ID:?Set the worker ID}"
: "${CODEX_CPA_BRIDGE_KEY:?Set the bridge Bearer key}"
exec /usr/local/bin/codex-app-server "$@"
