//! The real command host in a windowless process for `scripts/prove-external-wake.ps1`.
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::io;
use std::path::PathBuf;
use std::sync::Arc;

struct Windowless;
impl faber_host::Shell for Windowless {}

#[tokio::main(flavor = "current_thread")]
async fn main() -> io::Result<()> {
    let data = std::env::var_os("FABER_DATA_DIR").map(PathBuf::from).filter(|p| p.is_absolute());
    let Some(data) = data.filter(|p| p.join("external-wake-fixture").is_file()) else {
        return Err(io::Error::other("this program requires an isolated external-wake fixture"));
    };
    if !faber_host::setup::detect_clis().is_empty() {
        return Err(io::Error::other("model discovery must be disabled for this proof"));
    }
    faber_engine::tls::install();
    faber_channel::host::prepare()?;
    let host = faber_host::start(
        faber_host::StartOptions {
            data_dir: data.clone(),
            hidden: true,
            runtime: tokio::runtime::Handle::current(),
            installed_data_dir: None,
            game_guard: None,
            browser: None,
        },
        Arc::new(Windowless),
    )?;
    faber_channel::host::serve(&data, host).await
}
