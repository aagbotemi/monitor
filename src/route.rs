use std::sync::Arc;

use axum::{middleware, response::IntoResponse, routing::get, Extension, Json, Router};
use tower_http::trace::TraceLayer;

use crate::{
    handler::{auth::auth_handler, users::users_handler},
    middleware::auth,
    AppState,
};

pub fn create_router(app_state: Arc<AppState>) -> Router {
    let api_route = Router::new()
        .route("/healthchecker", get(health_checker_handler))
        .nest("/auth", auth_handler())
        .nest("/users", users_handler().layer(middleware::from_fn(auth)))
        .layer(TraceLayer::new_for_http())
        .layer(Extension(app_state));

    Router::new().nest("/api", api_route)
}

pub async fn health_checker_handler() -> impl IntoResponse {
    const MESSAGE: &str = "JWT Authentication in Rust using Axum, Postgres, and SQLX";

    let json_response = serde_json::json!({
        "status": "success",
        "message": MESSAGE
    });

    Json(json_response)
}
