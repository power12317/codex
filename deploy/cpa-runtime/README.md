# Codex Server deployment

Pushes to main publish `ghcr.io/power12317/codex-server:main` and `:latest`.
Use the service standalone or alongside CPA; a source checkout is not required
on a deployment host. The `deploy/cpa-runtime` directory name and compatibility
settings are retained so existing build scripts continue to work.

The build uses Rust 1.95.0, Debian bookworm and `Cargo.lock` (`--locked`). It builds
only `codex-app-server`, which contains the CPA master, credential workers, native
request builder, OAuth library and response bridge. The inference-only image
does not build or package the CLI, V8/Code Mode host or local execution helpers.
Both native Linux architectures must pass the startup check before either tag
is updated. The fork source commit is recorded in the image revision label.

## Standalone Linux deployment

Create `auths` and `codex-data` in a private deployment directory. For the image's
non-root default, make both writable by UID 10001, or explicitly select the UID
that owns them. The example below runs as UID 10001:

```sh
mkdir -p auths codex-data
sudo chown 10001:10001 auths codex-data
chmod 700 auths codex-data
```

Use this `compose.yaml` on a Linux host:

```yaml
services:
  codex-server:
    image: ghcr.io/power12317/codex-server:latest
    network_mode: host
    volumes:
      - ./auths:/cpa-auth
      - ./codex-data:/var/lib/codex
    restart: unless-stopped
```

Run `docker compose up -d` and check
`curl --fail http://127.0.0.1:38317/readyz`. An empty credential directory starts
the listener without account workers. Follow the
[third-party API guide](../../docs/codex-server-api.md) to provision credentials,
complete OAuth and submit inference. Any local application can connect directly;
CPA is not needed. For macOS/Windows development, use the
[native build instructions](../../docs/install.md) or an environment with Linux
host networking.

The master binds only to loopback. A Docker `ports:` mapping alone cannot reach a
listener bound to the container's `127.0.0.1`. Standalone host networking shares
that loopback with host applications; the CPA example below instead shares CPA's
network namespace.

## Existing CPA integration

The image runs one master and one native app-server child per enabled credential.
Add this fragment to CPA's Compose file; no additional environment,
command, bridge key, credential filename or port mapping is needed:

```yaml
services:
  codex-master:
    image: ghcr.io/power12317/codex-server:latest
    network_mode: service:cli-proxy-api
    user: "0:0"
    volumes:
      - ./auths:/cpa-auth
      - ./codex-data:/var/lib/codex
    restart: unless-stopped
```

Use the same runtime UID as CPA (the example matches a root CPA container).
The shared auth directory is `/cpa-auth`; private state is stored in the CPA
directory's `./codex-data`, mounted at `/var/lib/codex`.
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

Each credential worker holds logical session/thread mappings and bounded turn
routing contexts in memory. It does not create local conversation threads or
persist their message history. Root, child and parent identities are mapped
within the credential and CPA scope, and known response identity fields are
restored to the values CPA supplied. Explicit title requests use disposable
contexts and do not trigger additional title generation. Requests must include
their own history; worker restart clears turn state. Full native request/response
diagnostics still use the existing CPA log channel.

The master starts credential workers using the same `codex-app-server` binary.
OAuth uses the linked `codex-login` library. Enabled children retain applicable
native background activities. Refresh runs on credential access/401 recovery;
no independent account heartbeat or synthetic analytics event is added.

See the [third-party API](../../docs/codex-server-api.md), [v3 contract](../../codex-rs/app-server/docs/cpa-bridge-v3.md) and
[validation record](../../codex-rs/app-server/docs/cpa-bridge-validation.md).

Pushes to `main` run `.github/workflows/cpa-runtime-image.yml`.
The workflow builds on native Ubuntu amd64 and arm64 runners and checks `/readyz`
on each architecture before publishing a multiarch manifest under
`ghcr.io/power12317/codex-server:main` and `:latest`. Native images are
transferred between jobs by digest, without architecture or SHA image tags.
The source revision and image digests are recorded in the Actions summary.
A triggered workflow is not evidence of a successful publication. GHCR package
visibility and anonymous manifest access must be checked after publication.

## Remote applications

The service endpoint has no built-in client authentication or TLS. For access
from another machine, keep the listener on loopback and use an authenticated
transport. For example, when the service uses host networking on a Linux server:

```sh
ssh -N -L 38317:127.0.0.1:38317 user@server
```

The remote client then connects to `ws://127.0.0.1:38317/cpa/v1/ws` on its own
machine. An authenticated reverse proxy is another option: run it in a network
namespace that can reach the listener, enable WebSocket upgrades, preserve the
`/cpa/v1/ws` path, and set timeouts appropriate to inference streams. TLS and client
access control belong at that boundary. Possession of a `credentialId` is not a
client-authorization check; exposed clients can invoke the service's control API.

## Image-name migration and local builds

The repository is now `power12317/codex-server`, and new main-branch publications
use `ghcr.io/power12317/codex-server:main` and `:latest`. The previous
`ghcr.io/power12317/codex-cpa-runtime` package is a separate registry path; it is
not renamed automatically and this workflow no longer updates its tags. Wait for
the new package to publish successfully, then change only the Compose image
reference and recreate the service. Retain the existing credential and state
mounts. Container service names such as `codex-master` may stay unchanged in an
existing deployment.

Build locally from the repository root if the registry image is not available:

```sh
docker build -f deploy/cpa-runtime/Dockerfile \
  --build-arg FORK_REVISION="$(git rev-parse HEAD)" \
  -t codex-server:local .
```

Use `codex-server:local` in Compose for that build. The existing
`.github/workflows/cpa-runtime-image.yml` filename is retained; its repository
guard and image labels point to `power12317/codex-server`. Runtime executable,
credential filenames, state locations and protocol v3 identifiers are unchanged.

## Runtime timezone

The image does not select a timezone from a user's nationality, language, or
location. The published Debian runtime defaults to `Etc/UTC`. CPA is a separate
container; its timezone does not set the Codex worker timezone. To follow the
deployment host, pass its timezone configuration at deployment (for example,
read-only `/etc/localtime` and `/etc/timezone` mounts on hosts that provide both
files), without a conflicting `TZ` override. Docker does not inherit this
configuration automatically. No regional timezone is hardcoded in this image.

Verify the running container with `date '+%Z %z'`, `printenv TZ`,
`readlink /etc/localtime` and `cat /etc/timezone`. The inference adapter uses the
worker's effective local date/timezone for the current time context; it does not
replace historical dates, caller paths or caller OS descriptions.
