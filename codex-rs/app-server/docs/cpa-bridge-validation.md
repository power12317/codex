# CPA bridge validation record

## 2026-10-05 logical hierarchy and response identity mapping

Baseline: `17e357cce98ad3fe257a508ae55713c453f0e1b9`; official upstream remains
`afb436df8b70bb5bc57b86d9a3e829968988cd21`. These results use local HTTP mocks and
fake credentials. They do not establish real model-service acceptance or replay
of production CPA traffic.

- CPA unit and real-process integration tests: **37/37 passed** using
  `just test -p codex-app-server --lib --test all cpa_`.
- All app-server-protocol library tests and selected core inference/client tests:
  **360/360 passed** using
  `just test -p codex-app-server-protocol -p codex-core --lib -E 'package(codex-app-server-protocol) | test(client::inference_) | test(client::tests)'`.
- The experimental app-server schema was regenerated; stable and experimental
  schema fixture checks passed in the protocol run.
- Strict library Clippy passed for core, app-server and app-server-protocol with
  `-D warnings`; Rust formatting and `git diff --check` completed afterward.
- Hierarchy fixtures cover root, child and grandchild identity/header mapping,
  parent references, explicit subagent metadata, root cache affinity and separate
  routing state even when threads share a source turn ID.
- Two title requests get distinct synthesized turns and no previous routing
  state. Root state survives title calls. **Eight RPC requests produce exactly
  eight model POSTs**, without automatic title-generation requests. The worker
  state directory has no conversation rollout JSONL files.
- Response fixtures exercise source session/thread/cache restoration on each
  request, including changed cache keys on a reused context; absent source fields
  remove matching synthesized values. Structured HTTP errors are projected while
  their diagnostic copies retain native identities. Model/tool content,
  `response.id`, `call_id`, business metadata and opaque state are preserved.
- SSE tests cover every split position in a CRLF/multiline/UTF-8 fixture, lone CR,
  exact bytes for unchanged events, EOF fragments, immediate heartbeat forwarding,
  JSON bodies and the framing buffer limit.

The capability extension keeps protocol v3, `rawBody: true` and the existing
base64 body channel, and advertises `responseIdentityMapping: true`. Read-only
inspection of CPA's current Go adapter confirmed ordinary additive JSON decoding
and its existing v3/inference-only/rawBody checks; no CPA or CPAMP files changed.
The outer CPA session scope remains an isolation boundary. Only source values
actually received by Codex can be restored. Context state is worker-local and is
not restored after restart; requests still carry their own conversation history.
Linux builds and image smoke checks run in the dedicated image workflow after
push. These local results do not claim that a new image is already published.

## 2026-10-05 request normalization

Baseline: `9e40d2bca`, official source `afb436df8b70bb5bc57b86d9a3e829968988cd21`.
Validation uses fake credentials and local HTTP mocks, not a replay of business
requests or billable inference against the real service.

- CPA/master unit and real-process integration tests: **27/27 passed**.
- Native client, new identity/time/tool parser units, and all shared tool units:
  **147/147 passed**, including tool-continuation history preservation.
- Strict library Clippy passed for core, app-server and tools with `-D warnings`;
  Rust formatting and whitespace validation completed afterward.
- Both standard Responses and Responses Lite compare the prepared request with
  the actual HTTP body received by the mock. The sanitized incident fixture keeps
  six top-level tools: custom exec, two functions, two namespaces (eight nested
  functions), and web search. Namespace names, descriptions, schema `encrypted`,
  strict/defer-loading flags, custom grammar, `external_web_access: false`, and
  `search_content_types: [text, image]` survive native serialization. Lite uses
  native `additional_tools` placement. No caller tools array overwrites the
  builder's output.
- Namespace/call-ID history associations and exact returned SSE bytes are checked.
  Unsupported tool errors identify the failing array position.
- Session/thread/cache identity equality is checked in body and headers. Tests
  cover explicit turns, synthesized turns, same-session concurrency, credential
  and conversation isolation, same-value session/thread field fallback, first-state
  reuse, expiry with an in-flight holder,
  and capacity behavior without eviction of fresh state.
- UTC, Singapore and Tokyo are subprocess test configurations, not application
  defaults. All three replace only the current date/timezone context, preserving
  prior history, paths and OS declarations.
- The already-published ARM64 baseline image was run directly and returned
  `UTC +0000`, `/etc/localtime -> /usr/share/zoneinfo/Etc/UTC`, and `Etc/UTC` in
  `/etc/timezone`. Both published architecture configs have no `TZ` environment
  default. No regional timezone was added to the Dockerfile.

These checks establish local transport and serialization behavior. Acceptance of
all tool combinations by the real upstream service has not been exercised.
Synthetic turn continuity is worker-local and resets on restart; identity mapping
changes mean an upgrade uses different upstream conversation/cache IDs.

## v3 validation

Contract: V3-SEP-20260927. The current tests start a real directory master and
per-account app-server children against mock HTTP services. v2 results below are
historical and do not establish v3 correctness.

- All 11 CPA/master tests pass, including a truly tokenless placeholder followed
  by OAuth and immediate inference, 401 refresh into the original file, concurrent
  inference on one child, token-only reload without losing pending OAuth,
  disabling one of two accounts, and nested Unicode/uppercase-extension IDs.
- Raw-byte transport coverage preserves split UTF-8, comments, event/id lines,
  opaque non-JSON data and the original midstream transport error.
- The expanded run exercised 682 tests across app-server, protocol, API and
  transport. 681 passed including the corrected raw-byte header assertion.
  `codex-api::files::tests::upload_openai_file_reports_blob_transport_diagnostics_without_sas`
  remains failing locally at its existing `message.contains("failed after")`
  assertion. This file-upload implementation and test were not modified.
- CPA's real-process `TestCodexRuntimeForkV3OAuthAndModeSwitch` passed against
  the new binary: tokenless OAuth, immediate inference, 401 refresh, original-file
  persistence, actual logs, native routing after disable, and streaming after
  reenable. Coordinator log: `/tmp/cpa-master-v3-fork.log`.
- An additional 239 native client, login and TUI notification-routing tests passed.
- Final focused rerun: 12/12 master/bridge and raw-byte transport tests passed.
  Scoped `just fix` completed without warnings across the seven changed crates.
  `just fmt` and the final whitespace check completed afterward; tests were not
  rerun after fix/format, following repository instructions.
- Stable/experimental schema regeneration, entrypoint shellcheck, and workflow
  actionlint passed. No live credentials or billable inference were used.
- The initial debug build exhausted local disk. After it stopped, only this
  checkout's rebuildable Rust incremental cache was removed. Subsequent commands
  use `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`.

The native request boundary deliberately does not create agent turns; ordinary
agent-turn analytics/cost observations are therefore not synthesized. Enabled
runtime components retain their actual configured behavior, not an invented
account heartbeat. A disabled child is killed and reaped rather than gracefully
flushing account exporters. Already transmitted upstream work cannot be recalled.


Validated locally on macOS arm64, 2026-09-27, with Rust 1.95.0. Upstream baseline
`985cf47a4eb6084b2ff6b30ebdb1216acda85bb4`, branch `codex/cpa-managed-auth`.
No real OAuth credentials or billable model requests are used by the new tests.

Build/test environment used `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0
CARGO_PROFILE_TEST_DEBUG=0` after the first ordinary build exhausted disk space.
The code-mode companion uses official Codex V8 150.4.0 sandbox artifacts verified
against the repository's pinned release-manifest SHA-256, using
`deploy/cpa-runtime/fetch-v8.py`. No V8 version or upstream pin was changed.

## Historical v2 shared network namespace configuration

The CPA listener uses only `127.0.0.1` inside CPA's shared network namespace,
without a bridge key or Authorization header. Compose directly supplies the fixed
existing credential path, worker ID and port; no `.env` file, binding RPC,
worker-derived filename, rename or copy is introduced. AuthManager, OAuth state,
raw wire logs and the runtime lifecycle are unchanged.

**163/163 tests passed**: all 154 app-server transport tests plus 9 CPA tests,
using a client with no bridge authentication header. The existing `shared.json`
fixture deliberately differs from worker ID `worker`, and OAuth/refresh assertions
read tokens and preserved metadata from that same original file. Log:
`/tmp/cpa-loopback-tests.log`. Entrypoint shell syntax and Actions actionlint pass.
Earlier bearer-related validation below describes historical implementations only.

## Upstream logging extension and image workflow

The logging follow-up preserves the existing inference/auth flow and adds actual
prepared HTTP request, response and failure metadata. Validation covered:

- **326/326 passed**: 313 protocol tests, 4 raw event tests and 9 CPA tests,
  including the byte-tap test (`/tmp/cpa-body-tests.log`).
- Request logs are compared with the requests received by Wiremock, including
  method, target, headers and JSON body. Recovery logs are compared with every
  actual model POST rather than assuming a fixed retry count. Tests verify token
  SHA-256, header masking, response node extraction, and complete HTTP error
  bodies/metadata in notifications and RPC errors. Raw-body tests compare exact
  SSE bytes, including comments/event/id lines and UTF-8 split inside a codepoint,
  and verify that transport errors pass through unchanged.
- The coordinating CPA test `TestCodexRuntimeForkV2OAuthAndModeSwitch` passed with
  the new binary (`/tmp/cpa-runtime-logging-fork.log`): official OAuth, runtime
  inference and 401 recovery, native mode after switch-off, and runtime streaming
  after switch-on all produce records through the existing CPA logging path. The
  final raw-body run also passed with SSE comments/event/id preserved, attempt
  counts matched to real HTTP calls, and tokens masked.
- Stable/experimental schemas and generated Python/TypeScript exports were
  regenerated. Scoped app-server/protocol Clippy passed. Existing usage accounting
  was not changed. A read-only byte tap now supplies complete wire-body logs,
  while the official SSE parser and complete JSON event stream remain unchanged.

The branch image workflow builds `deploy/cpa-runtime/Dockerfile` on native amd64
and arm64 runners, checks `/readyz` for each image, and merges a multiarch manifest.
It publishes
`ghcr.io/power12317/codex-cpa-runtime:cpa-managed-auth` and `sha-<full commit>`, and
uses no QEMU emulation. Actions records the digest and
source SHA. Anonymous manifest access is verified separately from build success.
The early v2 local-only results below remain historical.

## V2 shared-file validation (initial milestone)

The v2 change starts from `8ee538eb3`. Tests use fake credentials, a real local TCP
WebSocket, and Wiremock; no live-account authorization or model billing was used.

- **714/714 passed** with `just test -p codex-login -p codex-app-server-transport
  -p codex-app-server-protocol -p codex-app-server -E 'package(codex-login) |
  package(codex-app-server-transport) | package(codex-app-server-protocol) |
  test(cpa_bridge)'`: 239 login tests, 154 transport tests, 313 protocol tests and
  8 CPA bridge tests. 1,820 nonselected/ignored tests were skipped. Log:
  `/tmp/cpa-v2-tests.log`.
- **4/4 raw event tests passed** with `just test -p codex-api -E
  'test(raw_responses)'`. These cover unknown/tool events, terminal fields,
  missing-terminal errors, long idle reads, cancellation and backpressure. Log:
  `/tmp/cpa-v2-raw-tests.log`.
- CPA tests cover exact request/event preservation, single-response inference,
  401 refresh persisted to the shared CPA file, unknown-field preservation, no
  mirror `auth.json`, callback PKCE correspondence, cross-connection pending
  login, invalid state and URL rejection without fetching the callback, Bearer
  authentication, the dedicated route and method whitelist, and owner changes
  cancelling old inference before reloading the shared credential.
- The coordinating CPA thread also ran its real v2 cross-repository test
  `TestCodexRuntimeForkV2OAuthAndModeSwitch` against this macOS binary and reported
  PASS (`/tmp/cpa-v2-fork3.log`): management OAuth start/callback across separate
  sockets, shared-file login, Codex inference/401 refresh, switching to native CPA
  with the refreshed token, then switching back to Codex streaming. No native
  mirror `auth.json` was created.
- Stable and experimental app-server schemas were regenerated. Only the
  precomputed experimental bundle changed. No Cargo dependency or config type
  changed, so Cargo/Bazel locks and config schema did not require updates.
- Scoped `just fix` completed for login, app-server transport, app-server protocol
  and app-server. The one test-helper `unwrap` warning was removed; the final
  app-server Clippy pass completed without warnings. `just fmt` completed. Tests
  preceded these final lint/format passes, following repository instructions.
- Final local `cargo build --locked -p codex-app-server --bin codex-app-server`
  completed after lint/format. Binary: `codex-rs/target/debug/codex-app-server`
  (macOS arm64), SHA-256 `ed8f0f0e59f541f35a32031ab49ae5f8d616975e30dbe3b08ee0ca85d4f34bd8`.
- The deploy entrypoint requires the fixed file, worker ID and bridge key env
  variables and starts the TCP worker automatically. Shell syntax was checked.
  Docker daemon queries did not respond within the bounded check; the Linux
  container image has **not been built or published** in this run.

The v2 implementation deliberately adds no cross-process locks, leases, epochs,
CAS or handoff protocol. An already-running token exchange is not guaranteed to
be mutually exclusive with owner switching. The unknown-field-preserving atomic
replacement is ordinary file persistence. No in-flight request-count API was added.

For review staging, the smallest coherent first stage is the login crate's shared
storage/manual OAuth foundation plus the transport endpoint helper. Those changes
leave the original app-server entry path in place until v2 integration lands. The
second stage connects the v2 RPCs, worker environment and lifecycle with the TCP
mock tests; the deploy entrypoint and contract/validation docs form a final stage.
The complete local change exceeds the usual 800-line target because it includes
all three stages and replaces the v1 integration fixtures.

The prior broader failures below remain historical and unassigned. This run did
not attempt to fix those unrelated areas or claim a full app-server regression.

## V1 results (historical)

- **10/10 CPA and lossless transport tests passed**:
  `just test -p codex-api -p codex-app-server -E 'test(cpa_bridge) | test(raw_responses)'`.
  Coverage includes full request equality, forced tools, unknown extensions and
  events, completed/incomplete/failed usage, long idle reads, bounded queues,
  cancellation/disconnect, account binding, opt-in/signed-out behavior, 503 and
  truncation with no replay, and managed 401 recovery with native token storage.
- **665/665 config and protocol tests passed**:
  `just test -p codex-config -p codex-app-server-protocol` (one ignored test).
- **TUI compatibility**: `just test -p codex-tui` ran 5,571 tests; 5,567 passed.
  Four cursor ANSI assertions failed because this execution environment sets
  `NO_COLOR=1`. All four passed when rerun with `env -u NO_COLOR` (eight other
  tests are skipped by the suite). No unrelated snapshot updates were accepted.
- The first broader API/config/protocol/app-server run executed 2,689 tests:
  2,626 passed, 62 failed, one timed out. After generating schemas and building
  the missing `codex-code-mode-host` and `test_stdio_server` helpers, the 60 failed
  API/app-server cases were rerun: 32 passed, 27 failed, one timed out. The three
  protocol failures passed after schema regeneration. The remaining failures are
  in untouched proxy/file-upload diagnostics, code-mode timing/prewarm and selected
  MCP/plugin capability tests. This is **not a green full app-server regression
  result**; no baseline comparison establishes causality. These remain upgrade/
  deployment validation items. Do not treat the successful CPA path as proof that
  every upstream test passes in this host environment.
- `just write-config-schema`, stable and `--experimental`
  `just write-app-server-schema` completed. The Python SDK generator needs a
  supported Python version; this run used `UV_PYTHON=3.13` because the generator
  does not recognize the host's Python 3.14. Generated SDK changes are included.
- Required scoped `just fix` completed for codex-api, codex-config,
  codex-app-server-protocol, codex-app-server and codex-tui. `just fmt` completed.
  Tests were run before this final lint/format pass, per repository instructions.
- `just bazel-lock-update` completed; the workspace-only dependency addition did
  not change `MODULE.bazel.lock`.
- The real CPA Go executor was exercised against this fork by the coordinating
  CPA task. It reported passing managed refresh/storage, raw tool/unknown events,
  streaming/nonstreaming, exactly one 503 POST, unsupported metadata rejection,
  cancellation closing the HTTP request, and persistent worker survival. Its
  validation record is maintained in the separate CPA repository.

The app-server and code-mode-host binaries are available under
`codex-rs/target/debug/`. The local executable is a macOS arm64 artifact. The
Dockerfile describes a Linux build with sibling runtime components and official
CLI login, but the container image has **not** been built or published here.

## Important fixture and protocol details

Managed fake JWTs need both a user identity and matching workspace identities.
AuthManager deliberately treats incomplete owner identity as an owner change on
credential refresh and revokes the previous network policy. The Rust fixture
therefore supplies `chatgpt_user_id` in addition to matching account claims and
`tokens.account_id`; this is a test fix, not a relaxation of managed policy.

Official unauthorized recovery first reloads native auth storage, then refreshes.
An unchanged reload may send the old credential once more: two rejected 401 POSTs
followed by one accepted POST are possible, with one refresh. Once upstream
accepts a request, no model retry or agent continuation occurs. Transport retry
configuration is zero because the upstream `max_attempts` field counts additional
attempts after the initial request.

## 2026-10-04 official main merge

Fetched the official `https://github.com/openai/codex.git` default branch `main`
at 2026-10-04 20:42:10 +08:00. The pinned merge input is
`afb436df8b70bb5bc57b86d9a3e829968988cd21` ("Honor server reasoning summary
defaults in new TUI threads", #50811), 356 commits after the previous upstream
baseline. The fork parent is `46f9fcc77397db2033bb010c15838684aa8c8719`.
This is an ordinary merge preserving both histories, not a release-tag rebase.

The five conflicts were generated artifacts: the two precomputed export bundles,
two TypeScript notification exports, and the config schema. They were regenerated
from the combined Rust types using `just write-app-server-schema`, its
`--experimental` variant, and `just write-config-schema`. The CPA notification
definitions are identical to the fork parent's definitions. `just bazel-lock-update`
succeeded and left the merged upstream lock unchanged; Bazel reported advisory
dependency-version/annotation warnings.

All 41 fork-added files survived. Aside from this validation record, the only
changed fork-added files are the current upstream provenance in the v3 document,
bridge capability constant, and Docker label. No compatibility source rewrite
was required for the upstream model catalog/provider changes, Responses Lite
tool-catalog construction, auth-storage telemetry, or Responses error handling.

Validation on the macOS arm64 development host:

- CPA tests plus all `codex-api`, `codex-app-server-protocol`, and `codex-login`
  tests: **792 passed, 1 failed** out of 793 selected tests (nextest run
  `3f1fc1b2-2e3b-41d9-96c4-b567d3b96251`). All selected CPA, protocol, and login
  tests passed, including manual OAuth, shared-file refresh/disable, readable-home
  migration, source-turn/concurrent/credential isolation, 30 historical tool-call
  pairs, original response bytes, and default-visible redacted logs.
- Native `client::` tests plus model-provider and models-manager unit tests:
  **192/192 passed** (run `195529f7-65d9-47bd-b954-391265c0f3cd`).
- `cargo check --locked -p codex-cli -p codex-code-mode-host --bins` passed.
  The default Deno V8 download returned HTTP 404; rerunning with the existing
  deployment fetcher's checksum-verified Codex V8 150.4.0 archive/binding pair
  succeeded. This uses the same artifact source already configured in Docker,
  without a V8 version change or a new download policy.
- Scoped `just fix` for app-server, core, codex-api, login and app-server-protocol
  completed successfully. It simplified a time-provider coercion while preserving
  its inferred type and removed one unused import in an upstream scenario test;
  neither change alters the CPA path or weakens an assertion.
- `just fmt` completed. The fork delta against the pinned official commit passes
  `git diff --check`. Checking the entire merge also reports trailing padding in
  11 upstream TUI snapshot files; each is byte-identical to upstream and retained
  as rendered snapshot content. The remaining staged files pass the whitespace
  check without exceptions.
- The sole API failure remains
  `files::tests::upload_openai_file_reports_blob_transport_diagnostics_without_sas`
  at `message.contains("failed after")`. An isolated rerun also failed (run
  `79b7bc2a-548a-40fd-bfd2-714274b0b8f3`). The same failure was recorded before
  this merge below, and its source file is unchanged by this upgrade. Its root
  cause remains unresolved; no assertion was removed or relaxed. This is not a
  claim that the complete API suite passed.

The protocol remains v3, with the existing TCP/WebSocket methods and port 38317.
Worker home names, legacy migration, source identity/turn state, no-turn-ID
fallback, inference-only behavior and CPA log correlation remain unchanged.
The dedicated `cpa-managed-auth`/immutable-SHA dual-architecture image workflow
is unchanged. No CPA/CPAMP files, deployment settings, extra ports/keys, capacity
policies, refresh locks, or server configurations were introduced or modified.
Linux image execution is delegated to that workflow; this merge does not claim
local Linux container validation or deployment.

## Remaining broader regression failures (historical)

These are the 27 failures and one timeout from the retry run. This table records
observations, not a claim that the baseline necessarily has the same failures.
No product changes were made to suppress them. Raw local logs are in
`/tmp/cpa-regression-retry.log` for this development session.

| Test | Observed failure / diagnostic |
| --- | --- |
| `suite::v2::code_mode_host::app_server_shares_flag_selected_grpc_code_mode_host_across_threads` | Equality assertion at `code_mode_host.rs:333` failed; root cause not established. |
| `suite::v2::account_system_proxy::browser_login_bootstraps_through_system_proxy` | Mock browser-login bootstrap received HTTP 502 from the token endpoint; root cause not established. |
| `suite::v2::code_mode_host::code_mode_model_output_uses_structured_host_timing::exec_grpc_host_only` | Timed out waiting for the `codex.code_mode.host_timing` JSON log event; root cause not established. |
| `suite::v2::code_mode_host::code_mode_model_output_uses_structured_host_timing::exec_grpc_with_overhead` | Timed out waiting for the `codex.code_mode.host_timing` JSON log event; root cause not established. |
| `suite::v2::executor_mcp::cached_executor_mcp_cannot_read_host_token_after_capability_downgrade` | Missing local `target/debug/codex` test executable; the standalone CLI was not built. |
| `suite::v2::executor_mcp::guardian_review_does_not_discover_executor_mcp` | Missing local `target/debug/codex` test executable; the standalone CLI was not built. |
| `suite::v2::executor_mcp::legacy_executor_blocks_plugin_host_credentials_and_keeps_host_owned_mcp` | Missing local `target/debug/codex` test executable; the standalone CLI was not built. |
| `suite::v2::executor_mcp::selected_executor_discovers_browser_mcp_with_executor_only_bearer_token` | Missing local `target/debug/codex` test executable; the standalone CLI was not built. |
| `suite::v2::executor_mcp::selected_executor_plugin_exposes_its_mcps_only_to_that_thread` | Missing local `target/debug/codex` test executable; the standalone CLI was not built. |
| `suite::v2::code_mode_host::code_mode_model_output_uses_structured_host_timing::wait_grpc_host_only` | Timed out waiting for the `codex.code_mode.host_timing` JSON log event; root cause not established. |
| `files::tests::upload_openai_file_reports_blob_transport_diagnostics_without_sas` | Transport diagnostic did not contain the expected `failed after` text; root cause not established. |
| `suite::v2::selected_capability_stack::managed_plugins_requirement_disables_selected_executor_plugin_capabilities::batched_executor_capability_discovery` | Missing local `target/debug/codex` test executable; the standalone CLI was not built. |
| `suite::v2::selected_capability_stack::managed_plugins_requirement_disables_selected_executor_plugin_capabilities::direct_selected_root_discovery` | Missing local `target/debug/codex` test executable; the standalone CLI was not built. |
| `suite::v2::selected_capability_stack::selected_capabilities_become_available_between_samples_in_one_turn::with_a_steered_plugin_mention` | Wiremock expected-request verification failed; root cause not established. |
| `suite::v2::selected_capability_stack::selected_capabilities_become_available_between_samples_in_one_turn::with_a_steered_server_mention` | Wiremock expected-request verification failed; root cause not established. |
| `suite::v2::selected_capability_stack::selected_capabilities_become_available_between_samples_in_one_turn::with_an_initial_plugin_mention` | Wiremock expected-request verification failed; root cause not established. |
| `suite::v2::selected_capability_stack::selected_capabilities_become_available_between_samples_in_one_turn::with_an_initial_plugin_mention_across_a_restart` | Wiremock expected-request verification failed; root cause not established. |
| `suite::v2::selected_capability_stack::selected_capabilities_become_available_between_samples_in_one_turn::with_an_initial_server_mention` | Wiremock expected-request verification failed; root cause not established. |
| `suite::v2::selected_capability_stack::selected_capabilities_become_available_between_samples_in_one_turn::without_a_mention` | Wiremock expected-request verification failed; root cause not established. |
| `suite::v2::selected_capability_stack::selected_plugin_mcp_startup_respects_explicit_mentions::optional_batched` | Missing local `target/debug/codex` test executable; the standalone CLI was not built. |
| `suite::v2::selected_capability_stack::selected_capability_stack_tracks_environment_availability_and_resume` | Wiremock expected-request verification failed; root cause not established. |
| `suite::v2::selected_capability_stack::selected_plugin_mcp_startup_respects_explicit_mentions::optional_direct` | Missing local `target/debug/codex` test executable; the standalone CLI was not built. |
| `suite::v2::selected_capability_stack::selected_plugin_mcp_startup_respects_explicit_mentions::plugin_link_batched` | Missing local `target/debug/codex` test executable; the standalone CLI was not built. |
| `suite::v2::selected_capability_stack::selected_plugin_mcp_startup_respects_explicit_mentions::structured_mention_batched` | Missing local `target/debug/codex` test executable; the standalone CLI was not built. |
| `suite::v2::selected_capability_stack::selected_plugin_mcp_startup_respects_explicit_mentions::plugin_link_direct` | Missing local `target/debug/codex` test executable; the standalone CLI was not built. |
| `suite::v2::selected_capability_stack::selected_plugin_mcp_startup_respects_explicit_mentions::structured_mention_direct` | Missing local `target/debug/codex` test executable; the standalone CLI was not built. |
| `suite::v2::code_mode_host::code_mode_model_output_uses_structured_host_timing::wait_grpc_with_overhead` | Timed out waiting for the `codex.code_mode.host_timing` JSON log event; root cause not established. |
| `suite::v2::code_mode_host::app_server_prewarms_flag_selected_grpc_code_mode_host_before_first_turn` | Test exceeded the 60-second nextest limit; root cause not established. |
