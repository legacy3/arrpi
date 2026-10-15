//! HTTP-only OAuth admission; resolved credentials never enter headers or responses.

use std::sync::Arc;

use arrpi_core::secret::Secret;
use axum::{
    Json,
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse as _, Response},
};
use secrecy::ExposeSecret as _;

use crate::{
    auth,
    error::{ApiError, HttpError},
    proxy::{self, Gateway},
};

pub(crate) async fn guard(
    State(gateway): State<Arc<Gateway>>,
    mut request: Request,
    next: Next,
) -> Response {
    if let Some(value) = request.headers_mut().get_mut(header::AUTHORIZATION) {
        value.set_sensitive(true);
    }

    if auth::authenticate(request.headers(), &gateway.clients).is_ok() {
        return proxy::admit(State(gateway), request, next)
            .await
            .into_response();
    }

    let Some(verifier) = &gateway.oauth else {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer")],
            Json(ApiError::UNAUTHORIZED.payload()),
        )
            .into_response();
    };
    let permit = match Arc::clone(&gateway.requests).try_acquire_owned() {
        Ok(permit) => permit,
        Err(error) => return HttpError(ApiError::from(error)).into_response(),
    };
    let resolved = verifier.client(presented(request.headers())).await;
    let client = match resolved {
        Ok(name) => gateway.clients.iter().find(|client| client.name == name),
        Err(refusal) => return refuse(refusal),
    };
    let Some(client) = client else {
        return HttpError(ApiError::FORBIDDEN).into_response();
    };
    let credential = Secret::from(client.credential.expose_secret().to_owned());

    request
        .extensions_mut()
        .insert(arrpi_mcp::AuthenticatedCredential::new(credential));
    request.headers_mut().remove(header::AUTHORIZATION);
    proxy::admit_reserved(permit, request, next).await
}

fn presented(headers: &HeaderMap) -> Option<&str> {
    let mut values = headers.get_all(header::AUTHORIZATION).iter();
    let value = values.next()?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    let token = token.trim_start_matches(' ');

    (scheme.eq_ignore_ascii_case("Bearer")
        && values.next().is_none()
        && token.len() <= 16 * 1024
        && !token.is_empty()
        && token.bytes().all(|byte| (0x21..=0x7e).contains(&byte)))
    .then_some(token)
}

fn refuse(refusal: arrpi_adapters::oauth::Refusal) -> Response {
    let status = StatusCode::from_u16(refusal.status).unwrap_or(StatusCode::UNAUTHORIZED);
    let error = if status == StatusCode::FORBIDDEN {
        ApiError::FORBIDDEN
    } else {
        ApiError::UNAUTHORIZED
    };
    let mut response = (status, Json(error.payload())).into_response();

    if let Some(challenge) = refusal
        .challenge
        .and_then(|value| HeaderValue::from_str(&value).ok())
    {
        response
            .headers_mut()
            .insert(header::WWW_AUTHENTICATE, challenge);
    }

    response
}
