//! Runtime credentials and per-source endpoint authorization.

pub(crate) mod oauth;

use arrpi_core::source::Principal;
use std::{collections::BTreeSet, ops::Deref};

use axum::http::{HeaderMap, header};
use secrecy::{ExposeSecret as _, SecretString};
use subtle::ConstantTimeEq as _;

use crate::{
    config::{Client, ConfigError},
    error::ApiError,
};

#[derive(Debug)]
pub(crate) struct Access {
    principal: Principal,
    credential: SecretString,
}

impl Access {
    pub(crate) fn new(client: &Client) -> Result<Self, ConfigError> {
        let credential = credential(client.key.value())?;

        if !arrpi_core::secret::valid_client_key(credential.expose_secret()) {
            return Err(ConfigError::CREDENTIAL);
        }

        Ok(Self {
            credential,
            principal: Principal {
                name: client.name.clone(),
                binding: client.seerr_user.clone(),
                tracking: client.tracking_user.clone(),
                permissions: client.permissions.iter().cloned().collect::<BTreeSet<_>>(),
            },
        })
    }

    pub(crate) fn observes_metrics(&self) -> bool {
        self.permissions.contains("admin") || self.permissions.contains("metrics")
    }

    pub(crate) fn same_credential(&self, other: &Self) -> bool {
        self.credential
            .expose_secret()
            .as_bytes()
            .ct_eq(other.credential.expose_secret().as_bytes())
            .into()
    }
}

pub(crate) fn authenticate<'a>(
    headers: &HeaderMap,
    clients: &'a [Access],
) -> Result<&'a Access, ApiError> {
    authenticate_key(bearer(headers)?, clients)
}

pub(crate) fn bearer(headers: &HeaderMap) -> Result<&str, ApiError> {
    let mut values = headers.get_all(header::AUTHORIZATION).iter();
    let value = values
        .next()
        .and_then(|value| value.to_str().ok())
        .ok_or(ApiError::UNAUTHORIZED)?;
    let (scheme, presented) = value.split_once(' ').ok_or(ApiError::UNAUTHORIZED)?;

    if !scheme.eq_ignore_ascii_case("Bearer") {
        return Err(ApiError::UNAUTHORIZED);
    }

    if values.next().is_some() || !arrpi_core::secret::valid_client_key(presented) {
        return Err(ApiError::UNAUTHORIZED);
    }

    Ok(presented)
}

pub(crate) fn credential(value: &str) -> Result<SecretString, ConfigError> {
    if value.is_empty()
        || value.len() > 128
        || !value.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
    {
        return Err(ConfigError::CREDENTIAL);
    }

    Ok(value.to_owned().into())
}

impl Deref for Access {
    type Target = Principal;
    fn deref(&self) -> &Self::Target {
        &self.principal
    }
}

pub(crate) fn authenticate_key<'a>(
    presented: &str,
    clients: &'a [Access],
) -> Result<&'a Access, ApiError> {
    let mut found = None;

    for client in clients {
        if bool::from(
            client
                .credential
                .expose_secret()
                .as_bytes()
                .ct_eq(presented.as_bytes()),
        ) {
            found = Some(client);
        }
    }

    found.ok_or(ApiError::UNAUTHORIZED)
}
