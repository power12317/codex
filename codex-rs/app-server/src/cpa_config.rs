//! Process-level CPA worker settings; request payloads never select a credential path.
use codex_core::config::Config;
use codex_login::AuthManager;
use codex_login::CpaCredentialFile;
use std::io;
use std::sync::Arc;

pub(crate) struct CpaConfig {
    pub(crate) file: CpaCredentialFile,
}

impl CpaConfig {
    pub(crate) fn from_env() -> io::Result<Option<Self>> {
        let Some(path) = std::env::var_os("CODEX_CPA_AUTH_FILE") else {
            return Ok(None);
        };
        let path = std::path::PathBuf::from(path);
        if !path.is_absolute() {
            return Err(io::Error::other("CODEX_CPA_AUTH_FILE must be absolute"));
        }
        let credential_id = std::env::var("CODEX_CPA_CREDENTIAL_ID")
            .ok()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| io::Error::other("CODEX_CPA_CREDENTIAL_ID is required for CPA"))?;
        Ok(Some(Self {
            file: CpaCredentialFile {
                path,
                credential_id,
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
