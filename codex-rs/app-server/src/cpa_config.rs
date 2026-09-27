//! Process-level CPA worker settings; request payloads never select a credential path.
use codex_core::config::Config;
use codex_login::AuthManager;
use codex_login::CpaCredentialFile;
use codex_websocket_auth::WebsocketAuthConfig;
use codex_websocket_auth::WebsocketAuthSettings;
use codex_websocket_auth::WebsocketCapabilityTokenSource;
use sha2::Digest;
use sha2::Sha256;
use std::io;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use std::sync::Arc;

pub(crate) struct CpaConfig {
    pub(crate) file: CpaCredentialFile,
    pub(crate) bind_address: SocketAddr,
    pub(crate) auth: WebsocketAuthSettings,
}

impl CpaConfig {
    pub(crate) fn from_env() -> io::Result<Option<Self>> {
        let Some(path) = std::env::var_os("CODEX_CPA_AUTH_FILE") else {
            return Ok(None);
        };
        let required = |name| {
            std::env::var(name)
                .ok()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| io::Error::other(format!("{name} is required for CPA")))
        };
        let path = std::path::PathBuf::from(path);
        if !path.is_absolute() {
            return Err(io::Error::other("CODEX_CPA_AUTH_FILE must be absolute"));
        }
        let worker_id = required("CODEX_CPA_WORKER_ID")?;
        let key = required("CODEX_CPA_BRIDGE_KEY")?;
        let port: u16 = std::env::var("CODEX_CPA_PORT")
            .unwrap_or_else(|_| "38317".into())
            .parse()
            .map_err(io::Error::other)?;
        if port == 18317 {
            return Err(io::Error::other("18317 belongs to CPAMP"));
        }
        Ok(Some(Self {
            file: CpaCredentialFile { path, worker_id },
            bind_address: (Ipv4Addr::LOCALHOST, port).into(),
            auth: WebsocketAuthSettings {
                config: Some(WebsocketAuthConfig::CapabilityToken {
                    source: WebsocketCapabilityTokenSource::TokenSha256 {
                        token_sha256: Sha256::digest(key.as_bytes()).into(),
                    },
                }),
            },
        }))
    }
}

pub(crate) async fn auth_manager(
    config: &Config,
    cpa: Option<&CpaConfig>,
) -> io::Result<Arc<AuthManager>> {
    if let Some(cpa) = cpa {
        Ok(AuthManager::shared_with_cpa_credentials(config, cpa.file.clone()).await)
    } else {
        AuthManager::shared_from_config(config, /*enable_codex_api_key_env*/ false)
            .await
            .map_err(io::Error::other)
    }
}
