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

The image runs one master and one native app-server child per enabled CPA
credential. Add this fragment to CPA's Compose file; no additional environment,
command, bridge key, credential filename or port mapping is needed:

```yaml
services:
  codex-master:
    image: ghcr.io/power12317/codex-cpa-runtime:cpa-managed-auth
    network_mode: service:cli-proxy-api
    user: "0:0"
    volumes:
      - ./auths:/cpa-auth
      - codex-runtime:/var/lib/codex
    restart: unless-stopped

volumes:
  codex-runtime:
```

Use the same runtime UID as CPA (the example matches a root CPA container).
The shared auth directory is `/cpa-auth`; private state is `/var/lib/codex`.
The loopback endpoint is `ws://127.0.0.1:38317/cpa/v1/ws`, with `/readyz` on that
same internal port. The existing CPA and gost services need no port changes.

`credentialId` is CPA's original path relative to the shared auth directory,
including nested directories, capitalization, spaces and Unicode. It is the
only account-routing identity. Each child has a persistent private CODEX_HOME
named after that original relative path, including the file extension:
`account.json` uses `/var/lib/codex/account.json/`, and `team/My Account.JSON`
uses `/var/lib/codex/team/My Account.JSON/`. A worker's optional configuration
file is directly inside that directory, for example
`/var/lib/codex/account.json/config.toml`; there is no extra `.codex` level.
When first starting an account after upgrading, the master moves its old hash
directory to the readable path if the latter does not exist, preserving all
state. If both paths exist, it uses the readable directory and leaves the old
one untouched. Credentials are still read and refreshed in the original shared
file. There is no mirrored auth.json.

Worker command lines still show `--listen stdio://`: their private configuration
is selected by `CODEX_HOME`, not a config-file command-line flag. `docker logs
codex-master` now includes a `cpa.worker.started` JSON event mapping the PID to
its credential name, CODEX_HOME and `config.toml` path. Request lifecycle, identity
mapping and upstream status/header events are visible by default on stderr,
without setting `RUST_LOG`. The RPC UUID and upstream `x-request-id` correlate
worker events with CPA's existing detailed logs; the RPC UUID is not CPA's
eight-character outer log ID. Authentication headers and cookies are redacted.

The master scans on startup and every 500 ms. It starts an account only when the
file has `type: "codex"`, `codex_cli.enabled: true`, and no `disabled: true`.
CPA's reload RPC uses the same reconciliation code and waits for starts/stops.
Token-only updates keep the same child. Disabling/removing a credential ends its
child without draining inference or flushing exporters; private state survives
reenabling. An already submitted upstream operation cannot be recalled.
CPA owns global/per-account preferences and request selection; the master only
applies effective file flags. The runtime does not execute model tool calls.

`codex`, `codex-app-server`, `exec-server`, `codex-code-mode-host` and the official
`bwrap` helper remain in the image. Enabled children retain applicable native
background activities. Refresh runs on credential access/401 recovery; no
independent account heartbeat or synthetic analytics event is added.

See the [v3 contract](../../codex-rs/app-server/docs/cpa-bridge-v3.md) and
[validation record](../../codex-rs/app-server/docs/cpa-bridge-validation.md).

Pushes to `codex/cpa-managed-auth` run `.github/workflows/cpa-runtime-image.yml`.
The workflow builds this Dockerfile on native Ubuntu amd64 and arm64 runners,
checks `/readyz` on each architecture, and merges a multiarch manifest for the dedicated
`ghcr.io/power12317/codex-cpa-runtime:cpa-managed-auth` tag and an immutable
`sha-<full commit>` tag.
It never updates `latest`. The image digest is recorded in the Actions summary.
A triggered workflow is not evidence of a successful publication. GHCR package
visibility and anonymous manifest access must be checked after publication.
