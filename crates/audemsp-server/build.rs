//! Build script for admin dashboard static files.
//!
//! Resolves the `dist/` directory path at compile time.
//! When `admin-dashboard` feature is enabled, sets `ADMIN_DIST_DIR`
//! environment variable for the `static_files.rs` module.

use std::path::PathBuf;

fn main() {
    // Only process when admin-dashboard feature is enabled
    #[cfg(feature = "admin-dashboard")]
    {
        // Resolve dist/ relative to workspace root
        let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
        let dist_dir = manifest_dir.join("../../www/apps/admin/dist");

        if dist_dir.exists() {
            println!("cargo:rustc-env=ADMIN_DIST_DIR={}", dist_dir.display());
            println!("cargo:rerun-if-changed=../../www/apps/admin/dist");
        } else {
            // ponytail: warn but don't fail — admin SPA is optional
            println!(
                "cargo:warning=admin dist/ not found at {} — run `pnpm build` in www/ first",
                dist_dir.display()
            );
            // Use a placeholder path that won't crash at runtime but will 404
            println!("cargo:rustc-env=ADMIN_DIST_DIR=/nonexistent/admin/dist");
        }
    }
    #[cfg(not(feature = "admin-dashboard"))]
    {
        // No-op when feature is disabled
    }
}
