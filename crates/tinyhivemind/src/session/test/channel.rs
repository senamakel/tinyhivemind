//! Channel-level projection tests: root-and-first-reply narrowing, per-root
//! promotion, and the window/scan bookkeeping that stops a channel read.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use super::support::{FakeLog, message, page, query};

#[tokio::test]
async fn filters_by_exact_desk_id_or_name_and_general_aliases() {
    let log = FakeLog::new(vec![page(
        vec![
            message(5, Some("Engineering"), None, "name"),
            message(4, Some("engineering"), None, "id"),
            message(3, Some("ENGINEERING"), None, "wrong case"),
        ],
        None,
    )]);
    let history = project_session(&log, &query(5)).await.expect("projects");
    assert_eq!(
        history
            .iter()
            .map(|item| item.content.as_str())
            .collect::<Vec<_>>(),
        vec!["id", "name"]
    );

    let general = FakeLog::new(vec![page(vec![message(2, None, None, "general")], None)]);
    let mut general_query = query(2);
    general_query.conversation.desk_id = "General".into();
    general_query.conversation.desk_name = "General".into();
    assert_eq!(
        project_session(&general, &general_query)
            .await
            .expect("general")
            .len(),
        1
    );
}

#[tokio::test]
async fn channel_projection_keeps_a_root_and_its_first_reply() {
    let log = FakeLog::new(vec![page(
        vec![
            message(4, Some("engineering"), Some(2), "second reply"),
            message(3, Some("engineering"), Some(2), "first reply"),
            message(2, Some("engineering"), None, "channel"),
        ],
        None,
    )]);
    let history = project_session(&log, &query(5)).await.expect("projects");
    assert_eq!(
        history
            .iter()
            .map(|item| item.content.as_str())
            .collect::<Vec<_>>(),
        vec!["channel", "first reply"]
    );
}

#[tokio::test]
async fn channel_projection_promotes_one_reply_per_root_independently() {
    let log = FakeLog::new(vec![page(
        vec![
            message(6, Some("engineering"), Some(2), "late on first"),
            message(5, Some("engineering"), Some(3), "answer two"),
            message(4, Some("engineering"), Some(2), "answer one"),
            message(3, Some("engineering"), None, "question two"),
            message(2, Some("engineering"), None, "question one"),
        ],
        None,
    )]);
    let history = project_session(&log, &query(9)).await.expect("projects");
    assert_eq!(
        history
            .iter()
            .map(|item| item.content.as_str())
            .collect::<Vec<_>>(),
        vec!["question one", "question two", "answer one", "answer two"]
    );
}

#[tokio::test]
async fn channel_projection_never_promotes_a_reply_to_a_reply() {
    let log = FakeLog::new(vec![page(
        vec![
            message(4, Some("engineering"), Some(3), "grandchild"),
            message(3, Some("engineering"), Some(2), "first reply"),
            message(2, Some("engineering"), None, "root"),
        ],
        None,
    )]);
    let history = project_session(&log, &query(5)).await.expect("projects");
    assert_eq!(
        history
            .iter()
            .map(|item| item.content.as_str())
            .collect::<Vec<_>>(),
        vec!["root", "first reply"]
    );
}

#[tokio::test]
async fn channel_projection_drops_a_reply_whose_root_is_outside_the_scan() {
    let log = FakeLog::new(vec![page(
        vec![message(9, Some("engineering"), Some(2), "orphan reply")],
        None,
    )]);
    assert!(
        project_session(&log, &query(5))
            .await
            .expect("projects")
            .is_empty()
    );
}

#[tokio::test]
async fn channel_projection_promotes_past_an_empty_first_reply_and_an_empty_root() {
    let log = FakeLog::new(vec![page(
        vec![
            message(5, Some("engineering"), Some(4), "reply to a blank root"),
            message(4, Some("engineering"), None, "  \n "),
            message(3, Some("engineering"), Some(1), "the reply that counts"),
            message(2, Some("engineering"), Some(1), " \t "),
            message(1, Some("engineering"), None, "root"),
        ],
        None,
    )]);
    let history = project_session(&log, &query(9)).await.expect("projects");
    assert_eq!(
        history
            .iter()
            .map(|item| item.content.as_str())
            .collect::<Vec<_>>(),
        vec!["root", "the reply that counts", "reply to a blank root"]
    );
}

#[tokio::test]
async fn channel_window_counts_what_survives_narrowing_not_rows_read() {
    let mut rows = vec![message(1, Some("engineering"), None, "root")];
    rows.extend((2..=40).map(|sequence| {
        message(
            sequence,
            Some("engineering"),
            Some(1),
            &format!("reply {sequence}"),
        )
    }));
    rows.reverse();
    let log = FakeLog::new(vec![page(rows, None)]);
    let history = project_session(&log, &query(2)).await.expect("projects");
    assert_eq!(
        history
            .iter()
            .map(|item| item.content.as_str())
            .collect::<Vec<_>>(),
        vec!["root", "reply 2"]
    );
}

#[tokio::test]
async fn channel_window_keeps_the_newest_survivors() {
    let log = FakeLog::new(vec![page(
        vec![
            message(4, Some("engineering"), Some(3), "newest reply"),
            message(3, Some("engineering"), None, "newest root"),
            message(2, Some("engineering"), Some(1), "oldest reply"),
            message(1, Some("engineering"), None, "oldest root"),
        ],
        None,
    )]);
    let history = project_session(&log, &query(2)).await.expect("projects");
    assert_eq!(
        history
            .iter()
            .map(|item| item.content.as_str())
            .collect::<Vec<_>>(),
        vec!["newest root", "newest reply"]
    );
}

#[tokio::test]
async fn channel_projection_stops_reading_once_the_window_is_met() {
    let log = FakeLog::new(vec![
        page(
            vec![
                message(4, Some("engineering"), Some(3), "reply"),
                message(3, Some("engineering"), None, "root"),
            ],
            Some(3),
        ),
        page(vec![message(2, Some("engineering"), None, "older")], None),
    ]);
    assert_eq!(
        project_session(&log, &query(2))
            .await
            .expect("projects")
            .len(),
        2
    );
    assert_eq!(log.call_count(), 1);
}

