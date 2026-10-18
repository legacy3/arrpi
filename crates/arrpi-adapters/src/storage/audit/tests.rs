//! History search covers every retained entry without crossing account boundaries.
use super::{AuditFilter, Store};
use arrpi_core::{contracts::AuditTarget, executor::AuditEvent};
use rusqlite::Connection;

#[test]
fn fuzzy_history_search_filters_all_pages_and_scopes_accounts() {
    let store = Store::new(Connection::open_in_memory().expect("database").into()).expect("store");

    for id in 1..=30 {
        let event = AuditEvent {
            tool: "refresh_media".to_owned(),
            target: AuditTarget {
                title: Some(format!("Äpfel im All {id}")),
                command_id: Some(7765 + id),
                source: Some("shows".to_owned()),
                ..Default::default()
            },
            outcome: if id % 2 == 0 { "queued" } else { "failed" }.to_owned(),
        };

        store
            .record_audit("friend", Some(1), &[event], 100)
            .expect("record");
    }

    let filter = AuditFilter {
        query: "Äpfl All",
        tool: Some("refresh_media"),
        outcome: Some("queued"),
        ..Default::default()
    };
    let first = store
        .search_audit("friend", 0, 10, &filter)
        .expect("first page");

    assert_eq!(first.total, 15);
    assert_eq!(first.items.len(), 10);
    assert_eq!(first.next_offset, Some(10));
    let last = store
        .search_audit("friend", 10, 10, &filter)
        .expect("last page");

    assert_eq!(last.items.len(), 5);
    assert_eq!(last.next_offset, None);
    assert_ne!(first.items[0].id, last.items[0].id);
    assert_eq!(
        store
            .search_audit("other", 0, 10, &filter)
            .expect("isolated")
            .total,
        0
    );
    let coordinate = store
        .search_audit(
            "friend",
            0,
            10,
            &AuditFilter {
                query: "7766",
                ..Default::default()
            },
        )
        .expect("coordinate");

    assert_eq!(coordinate.total, 1);
    assert_eq!(coordinate.items[0].target.command_id, Some(7766));
    let future = store
        .search_audit(
            "friend",
            0,
            10,
            &AuditFilter {
                since: Some("2099-01-01T00:00:00.000Z"),
                ..Default::default()
            },
        )
        .expect("future");

    assert_eq!(future.total, 0);
}

#[test]
fn history_search_has_unicode_and_page_bounds() {
    let store = Store::new(Connection::open_in_memory().expect("database").into()).expect("store");
    let text = "Ä".repeat(128);

    store
        .search_audit(
            "friend",
            0,
            10,
            &AuditFilter {
                query: &text,
                ..Default::default()
            },
        )
        .expect("valid search");
    let excessive = "Ä".repeat(129);

    store
        .search_audit(
            "friend",
            0,
            10,
            &AuditFilter {
                query: &excessive,
                ..Default::default()
            },
        )
        .expect_err("invalid search");
    store
        .search_audit("friend", 0, 1001, &AuditFilter::default())
        .expect_err("invalid search");
}
