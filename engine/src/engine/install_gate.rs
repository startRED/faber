//! The engine's side of installing an update: while the install is decided nothing new starts, and nothing running is cut.
use super::*;

/// The engine's admission closed for an install; it reopens when this drops, even on an unwind.
pub struct InstallGuard(Engine);
impl Drop for InstallGuard {
    fn drop(&mut self) {
        self.0.with(|c| c.installing = false);
    }
}

impl Engine {
    /// Closes admission for the install: refused while an answer runs or a message waits, and every new turn is refused
    /// from here on until the guard drops.
    pub fn install_admission(&self) -> Result<InstallGuard, String> {
        self.with(|c| {
            // An install the agent was allowed to make is work too, though Stop left nothing in the list.
            if c.installing || c.agent_installs > 0 || !c.running_items().is_empty() || c.convs.values().any(|v| !v.queued.is_empty()) {
                return Err("error.update.activeOperations".into());
            }
            c.installing = true;
            Ok(InstallGuard(self.clone()))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn install_reserves_chat_admission_and_releases_on_unwind() {
        let data = tempfile::tempdir().unwrap();
        let e = Engine::new(
            EngineOptions {
                data_dir: data.path().into(),
                clis: Default::default(),
                spawn: Arc::new(|_, _, _| panic!("install admission must not spawn")),
                windows: false,
                claude_env_billed: false,
                max_warm: 0,
                idle: Duration::from_secs(30),
                runtime: tokio::runtime::Handle::current(),
                ask: None,
            },
            Arc::new(|_| {}),
        )
        .unwrap();
        let request = || SendRequest {
            conversation_id: None,
            area: Area::Chat,
            project_id: None,
            text: "Hi".into(),
            images: vec![],
            files: vec![],
            choice: crate::router::Choice { vendor: "auto".into(), model: None, effort: None },
        };
        let install = e.install_admission().unwrap();
        assert!(e.install_admission().is_err());
        assert_eq!(e.send(request()), Err("error.update.activeOperations".into()));
        drop(install);
        assert_eq!(e.send(request()), Err("error.noSubscription".into()));
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _install = e.install_admission().unwrap();
            panic!("install crashed");
        }));
        assert!(e.install_admission().is_ok());
        let mut message = crate::model::Message::new("turn".into(), crate::model::Role::Assistant, "".into(), 1);
        message.status = Some(crate::model::Status::Streaming);
        e.with(|core| {
            let id = core.blank(Area::Chat, None);
            core.convs.get_mut(&id).unwrap().messages.push(message);
        });
        assert!(e.install_admission().is_err());
        e.shutdown();
    }
}
