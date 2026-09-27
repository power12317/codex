# CPA IPC v1 fork extension

Upstream baseline: `985cf47a4eb6084b2ff6b30ebdb1216acda85bb4`.
Branch: `codex/cpa-managed-auth`. IPC protocol version: **1**.

This extension uses the existing app-server Unix control socket and initialize /
initialized handshake with `capabilities.experimentalApi=true`. It is disabled by
default, and unavailable on TCP WebSocket/stdio. Enable explicitly:

```toml
[cpa_bridge]
enabled = true
credential_id = "worker-a"
```

Start with a private CODEX_HOME already managed by official Codex login:

```sh
cargo build -p codex-app-server --bin codex-app-server
CODEX_HOME=/private/path target/debug/codex-app-server --listen unix:///private/path/cpa.sock
```

On a machine with limited disk, use `CARGO_INCREMENTAL=0
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0` for builds/tests. Do not put
real credentials in fixtures. CPA owns only socket path, credential ID and expected
account ID; all OAuth refresh and native auth storage remain owned by AuthManager.
The app-server lifecycle, models refresh, and other enabled startup workers remain
unchanged. Disconnecting CPA only cancels its inference.

## Contract and supported scope

- `cpa/capabilities/read {}` returns protocolVersion, runtimeVersion,
  upstreamRevision, credentialId, accountId, authMode, executionMode, rawEvents,
  operations, persistentSessions. Only managed `Chatgpt` auth is accepted;
  signed out/API-key/external-token modes report null account/auth mode.
- `cpa/inference/start` takes requestId, credentialId, accountId, operation,
  sourceFormat, sessionId and a Responses request object. The source format is
  diagnostic provenance, not a request translation instruction.
- Only operation `responses`, with request `stream:true`, is supported. Transport
  upstream is official HTTP SSE; WebSocket inference and compact are not yet
  advertised. No persisted response/session state is supported.
- instructions, tools, tool_choice, input and unknown extension fields are sent
  as supplied. Function and custom tools are returned to the caller. Hosted tools
  are explicitly rejected. There is no thread/turn construction, tool dispatch,
  local shell/file/MCP access, or automatic model continuation.
- previous_response_id, conversation, generate, enabled background/store, and
  caller transport/identity metadata are rejected. Unsupported operations return
  an explicit error. Other unknown fields remain subject to upstream validation.
- The accepted result contains requestId, statusCode and safe response headers as
  arrays. It precedes `cpa/inference/event {requestId,event}` notifications.
  Complete JSON events, unknown fields, terminal details and usage are preserved.
  Events still pass through official SSE framing and response processing. The
  current upstream's safety_buffering metadata is informational and forwarded;
  any future upstream release introducing a release/buffering gate requires review
  before upgrading this extension.
- Only content-type, x-request-id, retry-after, openai-model and
  openai-processing-ms response headers cross IPC. Events containing credential
  headers in protocol header metadata fail closed instead of leaking credentials.
- Terminal `response.completed`, `response.incomplete`, `response.failed`, or
  `error` events are followed by `cpa/inference/completed {requestId}`. EOF without
  a terminal is an error. Pre-start errors use JSON-RPC error data.httpStatus;
  post-start errors use `cpa/inference/error`, followed by completed.
- `cpa/inference/cancel {requestId}` returns `{}`. One active inference per
  connection is enforced; cancellation is connection-scoped. Inference output is
  bounded by channels and awaits the socket writer. Dropping the receiver stops
  upstream parsing. No post-connect idle/read/total inference deadline is set.
  Only auth acquisition/recovery has a 30-second deadline. Normal Codex retains
  its configured idle timeout.
- No stream/network retry starts another inference. Only an upstream 401 before
  acceptance may trigger official AuthManager unauthorized recovery and retry.

## Upgrade gate and review stages

Every upstream update must record its new baseline SHA here and in capabilities.
Review `AuthManager`, workspace routing/HTTP factory, Responses endpoint and SSE
processing, safety buffering, response terminal rules, app-server initialization,
notification media processing, and disconnect lifecycle. Do not merely resolve
merge conflicts and assume compatibility. Run these mock-only acceptance gates:

1. Exact outbound request equality including forced function tools, custom tools,
   unknown fields and explicit rejection of unsupported session semantics.
2. Complete event order, unknown events, all terminal variants and usage; errors
   before headers, truncation and errors after acceptance.
3. No execution or auto-continuation after a model tool call; one upstream POST.
4. Opt-in and experimental gates, account mismatch/logout/switch, no secret IPC
   headers/logs, and managed refresh/recovery using fake token endpoints.
5. Cancel before and after acceptance, disconnect during blocked reads/writes,
   bounded output backpressure, long idle streams, and runtime survival.
6. Existing codex-api, config, protocol and app-server tests, generated config and
   experimental/stable protocol schemas, and Bazel lock verification.

The whole extension exceeds the repository's preferred logic-change size. Review
in dependent stages: (1) codex-api raw event facility and transport tests, (2) IPC
schema/config definitions and generated fixtures, (3) app-server bridge and Unix
socket integration tests. Stage 1 is the smallest coherent independently testable
change. No changes to codex-core agent behavior are needed.
