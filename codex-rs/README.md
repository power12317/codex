# Codex Server Rust workspace

The service entry point is `codex-app-server`. See the
[Codex Server overview](../README.md), [build guide](../docs/install.md), and
[third-party API reference](../docs/codex-server-api.md).

This workspace also retains the native Codex CLI, TUI and SDK components for
upstream maintenance. Their crate names and component documentation are unchanged.
[Upstream Codex CLI documentation](https://developers.openai.com/codex/cli)
describes the CLI, not the inference-only service listener.
