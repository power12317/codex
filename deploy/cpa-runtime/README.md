# Local CPA runtime image

Build from this repository root (no registry push is required):

```sh
docker build -f deploy/cpa-runtime/Dockerfile \
  --build-arg FORK_REVISION="$(git rev-parse HEAD)" \
  -t codex-cpa-runtime:local .
```

The build uses Rust 1.95.0, Debian bookworm and `Cargo.lock` (`--locked`).
`fetch-v8.py` downloads the official Codex-built sandbox archive/binding pair and
verifies both against the release manifest, then verifies that manifest against
the SHA-256 digest pinned in this repository, matching the upstream CI action. For
immutable deployment inputs pass `RUST_IMAGE` and `RUNTIME_IMAGE` with verified
image digests and pin the resulting runtime image digest in the CPA overlay.
Record the fork commit and any uncommitted patch separately. A tag and mutable apt
repositories alone do not promise byte-identical rebuilds. This Dockerfile has not
been built on the macOS development host; Linux container validation is required
before deployment. Budget sufficient disk for Rust and the V8 companion build.

The default entrypoint enables the v2 worker from environment variables:

```sh
CODEX_HOME=/var/lib/codex
CODEX_CPA_AUTH_FILE=/shared/auth/worker-a.json
CODEX_CPA_WORKER_ID=worker-a
CODEX_CPA_BRIDGE_KEY=<dedicated-bridge-key>
CODEX_CPA_PORT=38317
```

Mount CPA's auth directory at `/shared/auth` in both processes. Each worker also
needs its own `/var/lib/codex` volume for installation identity and runtime state.
The shared credential file remains in CPA's flat format and is the sole token
store; do not provision an independent `auth.json` mirror. The image runs as UID
10001, which needs read/write access to the shared file and its directory.

The listener is `ws://127.0.0.1:38317/cpa/v1/ws` and requires the bridge key in the
Authorization Bearer header. Additional workers use 38318, 38319, etc. Port 18317
belongs to CPAMP. A compose overlay can use `network_mode: service:cli-proxy-api`
so CPA and Codex share loopback networking. The image requires no socket volume.
The health endpoints are `/healthz` and `/readyz` on the same port.

Official `/usr/local/bin/codex`, `codex-app-server`, `exec-server`, and
`codex-code-mode-host` binaries remain in the image. The default entrypoint starts
the persistent app-server; CPA inference returns tool calls to the caller without
executing them. Use CPAMP → CPA → Codex browser OAuth and paste the callback URL
through CPAMP. Login writes the shared CPA credential file directly.

The worker uses tokens only while the file's `codex_cli.owner` is `codex` and its
`worker_id` matches. CPA controls the ownership flag and retained `enabled`
preference. Mode switches cancel old requests and reload authentication; no
cross-process refresh locking or epoch mechanism is provided.

See [the v2 IPC contract](../../codex-rs/app-server/docs/cpa-bridge-v2.md) and
[local validation](../../codex-rs/app-server/docs/cpa-bridge-validation.md).
