//! The one TLS crypto provider: ring, which the updater's rustls already compiles and installs. reqwest's own default,
//! aws-lc-rs, would add a C build of over two minutes to every cold build. reqwest is built without a provider, so a
//! process installs this one before its first client: the shell's `main` (the updater's client too), and the engine
//! before each fetch, for the runner and the tests that use it without the shell.

/// Makes ring the process's default rustls provider. Calling it again changes nothing: the first provider stays.
pub fn install() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}
