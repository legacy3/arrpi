//! Optional summary metadata cannot weaken ownership checks or invalidate receipts.

use super::receipt;
use arrpi_core::error::ApiError;
use serde_json::json;

#[test]
fn native_summary_metadata_is_projected_without_administrative_fields() {
    let native = json!({
        "id":7,"status":1,"requestedBy":{"id":1,"email":"private"},
        "createdAt":"2026-10-15T01:00:00Z","updatedAt":"2026-10-15T02:00:00Z",
        "type":"movie","is4k":false,
        "media":{"tmdbId":42,"title":"Ein Film","releaseDate":"2025-01-01","plexToken":"private"},
        "rootFolder":"private"
    });
    let result = receipt(&native, 1).expect("owned receipt");

    assert_eq!(result["title"], "Ein Film");
    assert_eq!(result["release_date"], "2025-01-01");
    assert_eq!(result["created_at"], "2026-10-15T01:00:00Z");
    assert!(!result.to_string().contains("private"));
    assert_eq!(receipt(&native, 2), Err(ApiError::FORBIDDEN));
}

#[test]
fn malformed_or_oversized_optional_metadata_preserves_known_receipts() {
    for media in [
        json!({"tmdbId":42}),
        json!({"tmdbId":42,"title":{},"releaseDate":2}),
        json!({"tmdbId":42,"title":"x".repeat(1025),"releaseDate":"x".repeat(65)}),
    ] {
        let native = json!({"id":7,"status":1,"requestedBy":{"id":1},"media":media,"type":"movie","is4k":false,"createdAt":{},"updatedAt":"x".repeat(65)});
        let result = receipt(&native, 1).expect("receipt remains authoritative");

        assert_eq!(result["request_id"], 7);
        assert!(result["title"].is_null());
        assert!(result["release_date"].is_null());
        assert!(result["created_at"].is_null());
        assert!(result["updated_at"].is_null());
    }
}
