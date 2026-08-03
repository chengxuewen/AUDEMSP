use axum::Router;

pub fn add_admin_routes(router: Router) -> Router {
    #[cfg(feature = "admin-dashboard")]
    {
        use axum::response::Html;
        use axum::routing::get;
        use tower_http::services::ServeDir;

        let dist = env!("ADMIN_DIST_DIR").to_string();
        let index = std::fs::read_to_string(std::path::Path::new(&dist).join("index.html"))
            .unwrap_or_else(|_| "<html><h1>SPA not built</h1></html>".to_string());
        let html = index.replace("src=\"/assets/", "src=\"/admin/assets/")
                        .replace("href=\"/assets/", "href=\"/admin/assets/");

        router
            .nest_service("/admin/assets", ServeDir::new(format!("{}/assets", dist)))
            .route("/admin", get(move || { let h = html.clone(); async move { Html(h) } }))
    }
    #[cfg(not(feature = "admin-dashboard"))]
    {
        router
    }
}
