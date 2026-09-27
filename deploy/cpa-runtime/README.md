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

Paths inside the image:

- `/usr/local/bin/codex-app-server`: default entrypoint, persistent app-server.
- `/usr/local/bin/codex`: official CLI, including managed OAuth login.
- `/usr/local/bin/exec-server` and `/usr/local/bin/codex-code-mode-host`: official
  sibling components retained for ordinary runtime behavior.
- `/var/lib/codex`: default `CODEX_HOME`, UID/GID 10001, private persistent volume.
- `/run/codex/cpa.sock`: default IPC socket, separate shared socket volume.

Each worker needs its own CODEX_HOME volume. Mount it only into that worker; CPA
shares only its socket volume. Authenticate using the official CLI with the same
worker volume (`--entrypoint /usr/local/bin/codex ... login`). No OAuth reimplementation,
token injection from CPA, or token projection directory is provided. Use the CLI's
normal interactive login options for the deployment environment.

Enable the bridge in that worker's `config.toml`:

```toml
[cpa_bridge]
enabled = true
credential_id = "worker-a"
```

Or pass the equivalent entrypoint arguments:

```sh
--listen unix:///run/codex/cpa.sock \
-c cpa_bridge.enabled=true \
-c 'cpa_bridge.credential_id="worker-a"'
```

The image does not enable the bridge automatically. Provision private volume
ownership for UID/GID 10001 and allow the CPA process to access the socket with
matching ownership; do not make the socket publicly writable. No TCP listener is
required. A compose overlay may join `network_mode: service:cli-proxy-api` to reuse
CPA networking without sharing its authentication files. Configure the CPA worker
with the stable credential ID and expected ChatGPT workspace account ID.

See [the IPC contract and upgrade gates](../../codex-rs/app-server/docs/cpa-bridge-v1.md).
