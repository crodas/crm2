use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

pub enum AppError {
    NotFound(String),
    BadRequest(String),
    InsufficientStock {
        product_id: i64,
        requested: f64,
        available: f64,
    },
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, body) = match self {
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, json!({"error": msg})),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, json!({"error": msg})),
            AppError::InsufficientStock {
                product_id,
                requested,
                available,
            } => (
                StatusCode::CONFLICT,
                json!({
                    "error": "insufficient_stock",
                    "product_id": product_id,
                    "requested": requested,
                    "available": available
                }),
            ),
            AppError::Internal(msg) => {
                tracing::error!("Internal error: {msg}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    json!({"error": "internal_error"}),
                )
            }
        };
        (status, axum::Json(body)).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::Internal(e.to_string())
    }
}
