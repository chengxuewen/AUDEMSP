//! Static file serving for the admin dashboard SPA.
//!
//! Only active when the `admin-dashboard` feature is enabled.
//! In production, the Vite-built `dist/` folder is served directly.
//! The `build.rs` script resolves the path at compile time.

use axum::Router;

/// Build a router that serves the admin SPA at `/admin`.
///
/// Feature-gated: returns an empty router when `admin-dashboard` is disabled.
pub fn admin_static_router() -> Router {
    #[cfg(feature = "admin-dashboard")]
    {
        use axum::routing::get_service;
        use tower_http::services::ServeDir;
        // ponytail: serve from dist/; build.rs ensures it exists or warns
        let serve_dir = ServeDir::new(env!("ADMIN_DIST_DIR"));
        Router::new().nest_service("/admin", get_service(serve_dir))
    }
    #[cfg(not(feature = "admin-dashboard"))]
    {
        Router::new()
    }
}
