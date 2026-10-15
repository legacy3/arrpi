//! HTTP rendering of protocol-independent failures.

use axum::{
    Json,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};

#[rustfmt::skip]
pub(crate) use arrpi_core::error::ApiError;

#[derive(Debug)]
pub(crate) struct HttpError(pub(crate) ApiError);

impl From<ApiError> for HttpError {
    fn from(error: ApiError) -> Self {
        Self(error)
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let error = self.0;
        let status =
            StatusCode::from_u16(error.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let mut response = (status, Json(error.payload())).into_response();

        response.headers_mut().insert(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static("private, no-store"),
        );

        if error == ApiError::UNAUTHORIZED {
            response.headers_mut().insert(
                header::WWW_AUTHENTICATE,
                header::HeaderValue::from_static("Bearer"),
            );
        }

        if matches!(error, ApiError::BUSY | ApiError::CAPACITY) {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, header::HeaderValue::from_static("1"));
        }

        response
    }
}
