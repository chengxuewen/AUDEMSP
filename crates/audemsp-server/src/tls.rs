//! Native TLS support via rustls.
//!
//! Loads PEM-encoded certificate chain and private key from config-specified paths.
//! Falls back to plain TCP when no TLS config is provided.

use axum_server::tls_rustls::RustlsConfig;
use audemsp_common::config::TlsConfig;
use audemsp_common::error::CoreError;
use std::path::Path;

/// Build a `RustlsConfig` from the shared `TlsConfig` paths.
///
/// `RustlsConfig::from_pem_file` is async — this function must be awaited.
pub async fn build_rustls_config(tls: &TlsConfig) -> Result<RustlsConfig, CoreError> {
    let cert_path = Path::new(&tls.cert_path);
    let key_path = Path::new(&tls.key_path);

    if !cert_path.exists() {
        return Err(CoreError::ConfigParse(format!(
            "TLS certificate file not found: {}",
            tls.cert_path
        )));
    }
    if !key_path.exists() {
        return Err(CoreError::ConfigParse(format!(
            "TLS private key file not found: {}",
            tls.key_path
        )));
    }

    RustlsConfig::from_pem_file(&tls.cert_path, &tls.key_path)
        .await
        .map_err(|e| CoreError::ConfigParse(format!("Failed to load TLS config: {e}")))
}
