use super::{Pane, rules::CALL};
use serde_json::{Value, json};

impl Pane {
    pub fn supports_extensions(&self) -> bool {
        self.driver.supports_extensions()
    }

    pub(super) async fn sync_extensions(&self, enabled: bool) -> Result<(), String> {
        tokio::time::timeout(CALL, self.driver.call(self.view, "Faber.extensionMode", json!({ "enabled": enabled })))
            .await
            .map_err(|_| "error.browser.extensionsFailed")??;
        Ok(())
    }

    pub async fn extension_mode(&self, enabled: bool, save: impl FnOnce() -> Result<(), String>) -> Result<(), String> {
        if !self.supports_extensions() {
            return Err("error.browser.extensionsUnavailable".into());
        }
        let _gate = self.extension_gate.lock().await;
        save()?;
        self.close();
        self.sync_extensions(enabled).await.map_err(|_| "error.browser.extensionsFailed".into())
    }

    pub async fn extensions_available(&self) -> Result<(), String> {
        if !self.supports_extensions() {
            return Err("error.browser.extensionsUnavailable".into());
        }
        if !self.home.extensions_enabled().await {
            return Err("error.browser.extensionsDisabled".into());
        }
        Ok(())
    }

    pub async fn extensions(&self, scope: &str, action: &str, folder: Option<&str>) -> Result<Value, String> {
        let _gate = self.extension_gate.lock().await;
        self.extensions_available().await?;
        self.sync_extensions(true).await?;
        self.driver.call(self.view, "Faber.extensions", json!({ "scope": scope, "action": action, "folder": folder })).await
    }

    pub(super) async fn forget_extensions(&self, scope: String) {
        let _gate = self.extension_gate.lock().await;
        let _ = self.driver.call(self.view, "Faber.forgetExtensions", json!({ "scope": scope })).await;
    }
}
