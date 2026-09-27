# CPA bridge validation record

Validated locally on macOS arm64, 2026-09-27, with Rust 1.95.0. Upstream baseline
`985cf47a4eb6084b2ff6b30ebdb1216acda85bb4`, branch `codex/cpa-managed-auth`.
No real OAuth credentials or billable model requests are used by the new tests.

Build/test environment used `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0
CARGO_PROFILE_TEST_DEBUG=0` after the first ordinary build exhausted disk space.
The code-mode companion uses official Codex V8 150.4.0 sandbox artifacts verified
against the repository's pinned release-manifest SHA-256, using
`deploy/cpa-runtime/fetch-v8.py`. No V8 version or upstream pin was changed.

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

## Remaining broader regression failures

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
