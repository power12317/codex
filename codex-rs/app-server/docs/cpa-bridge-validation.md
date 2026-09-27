# CPA bridge validation record

Validated locally on macOS arm64, 2026-09-27, with Rust 1.95.0. Upstream baseline
`985cf47a4eb6084b2ff6b30ebdb1216acda85bb4`, branch `codex/cpa-managed-auth`.
No real OAuth credentials or billable model requests are used by the new tests.

Build/test environment used `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0
CARGO_PROFILE_TEST_DEBUG=0` after the first ordinary build exhausted disk space.
The code-mode companion uses official Codex V8 150.4.0 sandbox artifacts verified
against the repository's pinned release-manifest SHA-256, using
`deploy/cpa-runtime/fetch-v8.py`. No V8 version or upstream pin was changed.

## Results

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
