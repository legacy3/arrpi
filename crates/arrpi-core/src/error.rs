//! Stable redacted application errors with an opaque implementation kind.
use serde_json::Value;

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
enum ErrorKind {
    #[error("batch time budget expired; this read can be retried")]
    BatchTimeout,
    #[error("{0}")]
    Configuration(crate::config::ConfigError),
    #[error("upstream queue timed out before dispatch; retry is safe")]
    QueueTimeout,
    #[error("Seerr request quota exceeded")]
    QuotaExceeded,
    #[error("Seerr write contract is unverified; read tools remain available")]
    UnsupportedVersion,
    #[error("unknown_outcome; inspect media details and your requests before resubmitting")]
    UnknownOutcome,
    #[error(
        "unknown_outcome; Seerr created request #{request_id} with a different profile; inspect request_status before resubmitting"
    )]
    ProfileMismatch { request_id: u64 },
    #[error("profile is not selectable; use request_profiles for the current choices")]
    ProfileUnavailable,
    #[error("automatic search requires a quality profile limited to 1080p or below")]
    UnsafeSearchProfile,
    #[error("native maintenance requires stable Sonarr 4 or Radarr 3 through 6")]
    UnsupportedMaintenanceVersion,
    #[error(
        "search exceeds configured episode limits; narrow its scope or adjust MCP search/batch limits"
    )]
    SearchTooLarge,
    #[error("media is not tracked by Seerr; request it first")]
    MediaNotTracked,
    #[error("only pending non-4K requests can be cancelled")]
    RequestNotPending,
    #[error("configuration changed; reload before saving")]
    Conflict,
    #[error("authentication required")]
    Unauthorized,
    #[error("permission denied")]
    Forbidden,
    #[error("requested resource or endpoint was not found")]
    NotFound,
    #[error("invalid query parameters")]
    BadQuery,
    #[error("request capacity exceeded")]
    Busy,
    #[error("source capacity exceeded")]
    Capacity,
    #[error("upstream unavailable")]
    Unavailable,
    #[error("upstream timed out")]
    Timeout,
    #[error("upstream rejected the query parameters")]
    UpstreamBadQuery,
    #[error("upstream rate limit reached; retry after waiting")]
    UpstreamRateLimited,
    #[error("upstream rejected request")]
    Rejected,
    #[error(
        "upstream response exceeds cache.max_body_bytes; raise the configured limit for a larger library"
    )]
    ResponseTooLarge,
    #[error("combined calendar exceeds 8192 events; request a shorter date range")]
    CalendarTooLarge,
    #[error("invalid upstream response")]
    InvalidResponse,
    #[error("invalid upstream JSON at line {line}, column {column}")]
    InvalidJson { line: usize, column: usize },
    #[error("calendar processing interrupted")]
    Interrupted,
    #[error("internal processing failed")]
    Internal,
}

/// A typed failure preserves mutation uncertainty without exposing implementation variants.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error(transparent)]
pub struct ApiError(ErrorKind);

impl ApiError {
    pub const BATCH_TIMEOUT: Self = Self(ErrorKind::BatchTimeout);
    pub const QUEUE_TIMEOUT: Self = Self(ErrorKind::QueueTimeout);
    pub const QUOTA_EXCEEDED: Self = Self(ErrorKind::QuotaExceeded);
    pub const UNSUPPORTED_VERSION: Self = Self(ErrorKind::UnsupportedVersion);
    pub const UNKNOWN_OUTCOME: Self = Self(ErrorKind::UnknownOutcome);
    pub const PROFILE_UNAVAILABLE: Self = Self(ErrorKind::ProfileUnavailable);
    pub const UNSAFE_SEARCH_PROFILE: Self = Self(ErrorKind::UnsafeSearchProfile);
    pub const UNSUPPORTED_MAINTENANCE_VERSION: Self =
        Self(ErrorKind::UnsupportedMaintenanceVersion);
    pub const SEARCH_TOO_LARGE: Self = Self(ErrorKind::SearchTooLarge);
    pub const MEDIA_NOT_TRACKED: Self = Self(ErrorKind::MediaNotTracked);
    pub const REQUEST_NOT_PENDING: Self = Self(ErrorKind::RequestNotPending);
    pub const CONFLICT: Self = Self(ErrorKind::Conflict);
    pub const UNAUTHORIZED: Self = Self(ErrorKind::Unauthorized);
    pub const FORBIDDEN: Self = Self(ErrorKind::Forbidden);
    pub const NOT_FOUND: Self = Self(ErrorKind::NotFound);
    pub const BAD_QUERY: Self = Self(ErrorKind::BadQuery);
    pub const BUSY: Self = Self(ErrorKind::Busy);
    pub const CAPACITY: Self = Self(ErrorKind::Capacity);
    pub const UNAVAILABLE: Self = Self(ErrorKind::Unavailable);
    pub const TIMEOUT: Self = Self(ErrorKind::Timeout);
    pub const UPSTREAM_BAD_QUERY: Self = Self(ErrorKind::UpstreamBadQuery);
    pub const UPSTREAM_RATE_LIMITED: Self = Self(ErrorKind::UpstreamRateLimited);
    pub const REJECTED: Self = Self(ErrorKind::Rejected);
    pub const RESPONSE_TOO_LARGE: Self = Self(ErrorKind::ResponseTooLarge);
    pub const CALENDAR_TOO_LARGE: Self = Self(ErrorKind::CalendarTooLarge);
    pub const INVALID_RESPONSE: Self = Self(ErrorKind::InvalidResponse);
    pub const INTERRUPTED: Self = Self(ErrorKind::Interrupted);
    pub const INTERNAL: Self = Self(ErrorKind::Internal);

    /// Preserve a static configuration rule without retaining rejected values.
    #[must_use]
    pub const fn configuration(error: crate::config::ConfigError) -> Self {
        Self(ErrorKind::Configuration(error))
    }

    /// Record a created request whose accepted profile did not match the user's choice.
    #[must_use]
    pub const fn profile_mismatch(request_id: u64) -> Self {
        Self(ErrorKind::ProfileMismatch { request_id })
    }

    /// Preserve safe JSON coordinates without retaining the native body or origin.
    #[must_use]
    pub const fn invalid_json(line: usize, column: usize) -> Self {
        Self(ErrorKind::InvalidJson { line, column })
    }

    /// Map a complete native read status to the public error vocabulary.
    #[must_use]
    pub const fn upstream_read(status: u16) -> Self {
        Self(match status {
            400 => ErrorKind::UpstreamBadQuery,
            401 | 403 => ErrorKind::Forbidden,
            404 => ErrorKind::NotFound,
            409 => ErrorKind::Conflict,
            429 => ErrorKind::UpstreamRateLimited,
            500..=599 => ErrorKind::Unavailable,
            _ => ErrorKind::Rejected,
        })
    }

    /// Decode only known public errors from a complete trusted `ArrPi` response.
    #[must_use]
    pub fn from_payload(value: &Value) -> Option<Self> {
        match value.get("code")?.as_str()? {
            "unknown_outcome" => Some(
                value
                    .get("request_id")
                    .and_then(Value::as_u64)
                    .filter(|id| *id > 0)
                    .map_or(Self::UNKNOWN_OUTCOME, Self::profile_mismatch),
            ),

            "invalid_upstream_json" => Some(Self::invalid_json(
                json_position(value, "line")?,
                json_position(value, "column")?,
            )),

            code => PUBLIC_ERRORS
                .iter()
                .copied()
                .find(|error| error.code() == code),
        }
    }

    /// Whether a read failure permits serving an explicitly stale cached result.
    #[must_use]
    pub const fn can_serve_stale(self) -> bool {
        matches!(self.0, ErrorKind::Unavailable | ErrorKind::Timeout)
    }

    /// Return the HTTP status preserving definite and indeterminate outcomes.
    #[must_use]
    pub const fn status(self) -> u16 {
        match self.0 {
            ErrorKind::BatchTimeout => 504,
            ErrorKind::Conflict | ErrorKind::RequestNotPending => 409,
            ErrorKind::Unauthorized => 401,
            ErrorKind::Forbidden => 403,
            ErrorKind::NotFound | ErrorKind::MediaNotTracked => 404,
            ErrorKind::BadQuery
            | ErrorKind::ProfileUnavailable
            | ErrorKind::UnsafeSearchProfile
            | ErrorKind::SearchTooLarge
            | ErrorKind::Configuration(_) => 400,
            ErrorKind::Busy
            | ErrorKind::QueueTimeout
            | ErrorKind::QuotaExceeded
            | ErrorKind::UpstreamRateLimited => 429,
            ErrorKind::Capacity
            | ErrorKind::Interrupted
            | ErrorKind::UnsupportedVersion
            | ErrorKind::UnsupportedMaintenanceVersion => 503,
            ErrorKind::Internal => 500,
            ErrorKind::UnknownOutcome
            | ErrorKind::ProfileMismatch { .. }
            | ErrorKind::Unavailable
            | ErrorKind::Timeout
            | ErrorKind::UpstreamBadQuery
            | ErrorKind::ResponseTooLarge
            | ErrorKind::CalendarTooLarge
            | ErrorKind::Rejected
            | ErrorKind::InvalidResponse
            | ErrorKind::InvalidJson { .. } => 502,
        }
    }

    /// Return the stable, redacted wire error code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self.0 {
            ErrorKind::BatchTimeout => "batch_timeout",
            ErrorKind::Configuration(_) => "invalid_configuration",
            ErrorKind::ProfileUnavailable => "profile_not_selectable",
            ErrorKind::UnsafeSearchProfile => "unsafe_search_profile",
            ErrorKind::UnsupportedMaintenanceVersion => "maintenance_disabled_unverified_version",
            ErrorKind::SearchTooLarge => "search_too_large",
            ErrorKind::UpstreamBadQuery => "upstream_invalid_arguments",
            ErrorKind::UpstreamRateLimited => "upstream_rate_limited",
            ErrorKind::ResponseTooLarge => "upstream_response_too_large",
            ErrorKind::CalendarTooLarge => "calendar_too_large",
            ErrorKind::InvalidResponse => "invalid_upstream_response",
            ErrorKind::InvalidJson { .. } => "invalid_upstream_json",
            ErrorKind::Internal => "internal_error",
            ErrorKind::MediaNotTracked => "media_not_tracked_request_first",
            ErrorKind::RequestNotPending => "request_not_pending",
            ErrorKind::QueueTimeout => "queue_timeout_retry_safe",
            ErrorKind::QuotaExceeded => "quota_exceeded",
            ErrorKind::UnsupportedVersion => "writes_disabled_unverified_version",
            ErrorKind::UnknownOutcome | ErrorKind::ProfileMismatch { .. } => "unknown_outcome",
            ErrorKind::Forbidden => "permission_denied",
            ErrorKind::Unauthorized => "authentication_required",
            ErrorKind::BadQuery => "invalid_arguments",
            ErrorKind::NotFound => "not_found",
            ErrorKind::Busy | ErrorKind::Capacity => "busy",
            ErrorKind::Unavailable | ErrorKind::Timeout => "upstream_unavailable",
            ErrorKind::Rejected => "upstream_rejected",
            ErrorKind::Conflict => "configuration_changed",
            ErrorKind::Interrupted => "processing_interrupted",
        }
    }

    /// Render the public error document without native bodies or origins.
    #[must_use]
    pub fn payload(self) -> Value {
        let mut value = serde_json::json!({"error":self.to_string(),"code":self.code()});

        if self == Self::BATCH_TIMEOUT {
            value["retry_safe"] = serde_json::json!(true);
        }

        if let ErrorKind::Configuration(error) = self.0 {
            value["reason"] = serde_json::json!(error.reason());
        }

        if matches!(
            self.0,
            ErrorKind::UnknownOutcome | ErrorKind::ProfileMismatch { .. }
        ) {
            value["follow_up"] = serde_json::json!(
                "Use request_status, media_details and my_requests to reconcile before resubmitting."
            );
        }

        if let ErrorKind::InvalidJson { line, column } = self.0 {
            value["line"] = serde_json::json!(line);
            value["column"] = serde_json::json!(column);
        }

        if let ErrorKind::ProfileMismatch { request_id } = self.0 {
            value["request_id"] = serde_json::json!(request_id);
            value["profile_applied"] = serde_json::json!(false);
        }

        value
    }
}

impl From<tokio::task::JoinError> for ApiError {
    fn from(error: tokio::task::JoinError) -> Self {
        if error.is_cancelled() {
            Self::INTERRUPTED
        } else {
            Self::INTERNAL
        }
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(error: serde_json::Error) -> Self {
        if error.is_data() {
            Self::INVALID_RESPONSE
        } else {
            Self::invalid_json(error.line(), error.column())
        }
    }
}

impl From<tokio::sync::TryAcquireError> for ApiError {
    fn from(error: tokio::sync::TryAcquireError) -> Self {
        match error {
            tokio::sync::TryAcquireError::Closed => Self::UNAVAILABLE,
            tokio::sync::TryAcquireError::NoPermits => Self::BUSY,
        }
    }
}

fn json_position(value: &Value, key: &str) -> Option<usize> {
    value
        .get(key)?
        .as_u64()
        .and_then(|number| usize::try_from(number).ok())
}

const PUBLIC_ERRORS: &[ApiError] = &[
    ApiError::BATCH_TIMEOUT,
    ApiError::UPSTREAM_BAD_QUERY,
    ApiError::UPSTREAM_RATE_LIMITED,
    ApiError::RESPONSE_TOO_LARGE,
    ApiError::CALENDAR_TOO_LARGE,
    ApiError::UNAVAILABLE,
    ApiError::REJECTED,
    ApiError::PROFILE_UNAVAILABLE,
    ApiError::UNSAFE_SEARCH_PROFILE,
    ApiError::UNSUPPORTED_MAINTENANCE_VERSION,
    ApiError::SEARCH_TOO_LARGE,
    ApiError::QUEUE_TIMEOUT,
    ApiError::QUOTA_EXCEEDED,
    ApiError::UNSUPPORTED_VERSION,
    ApiError::REQUEST_NOT_PENDING,
    ApiError::MEDIA_NOT_TRACKED,
    ApiError::FORBIDDEN,
    ApiError::UNAUTHORIZED,
    ApiError::BAD_QUERY,
    ApiError::NOT_FOUND,
    ApiError::BUSY,
    ApiError::INVALID_RESPONSE,
    ApiError::INTERNAL,
    ApiError::CONFLICT,
    ApiError::INTERRUPTED,
];
