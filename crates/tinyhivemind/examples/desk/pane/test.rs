//! Unit tests for the watched desk: the window it builds and the feed it reads.
//!
//! Neither needs tmux or a server. The layout is built as data before it is
//! run, and the feed is a file of `data:` frames, so both halves of this
//! module test the interesting decisions without a terminal in sight.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use super::{
    events::Watch,
    tail,
    tmux::{self, Attach},
};

/// Three seats, as the room is watched.
fn three() -> Vec<Attach> {
    ["theory", "solver", "checker"]
        .into_iter()
        .enumerate()
        .map(|(index, seat)| Attach {
            seat: seat.to_string(),
            base: format!("http://127.0.0.1:{}", 4830 + index),
        })
        .collect()
}

/// Feed one frame per line into a watch.
fn watch(frames: &[serde_json::Value]) -> Watch {
    let mut watch = Watch::default();
    for frame in frames {
        watch.absorb(&format!("data: {frame}"));
    }
    watch
}

#[test]
fn opens_one_session_and_splits_it_once_per_further_seat() {
    let plan = tmux::layout("desk", "/ws", None, &three());
    let openers: Vec<&str> = plan
        .iter()
        .filter_map(|argv| argv.first().map(String::as_str))
        .filter(|verb| *verb == "new-session" || *verb == "split-window")
        .collect();
    assert_eq!(openers, vec!["new-session", "split-window", "split-window"]);
}

#[test]
fn stacks_the_panes_rather_than_letting_tmux_halve_them() {
    let plan = tmux::layout("desk", "/ws", None, &three());
    for argv in &plan {
        if argv.first().map(String::as_str) == Some("split-window") {
            assert!(
                argv.iter().any(|arg| arg == "-v"),
                "{argv:?} is not vertical"
            );
        }
    }
    assert!(
        plan.iter().any(
            |argv| argv.first().is_some_and(|verb| verb == "select-layout")
                && argv.last().is_some_and(|layout| layout == "even-vertical")
        ),
        "the splits are never evened out"
    );
}

#[test]
fn splits_a_named_pane_so_pane_order_is_seat_order() {
    let plan = tmux::layout("desk", "/ws", None, &three());
    let targets: Vec<String> = plan
        .iter()
        .filter(|argv| argv.first().is_some_and(|verb| verb == "split-window"))
        .filter_map(|argv| {
            let at = argv.iter().position(|arg| arg == "-t")?;
            argv.get(at + 1).cloned()
        })
        .collect();
    // Splitting the window rather than a named pane splits whichever pane is
    // active, which puts the third seat between the first two.
    assert_eq!(targets, vec!["desk:desk.0", "desk:desk.1"]);
}

#[test]
fn titles_every_pane_with_the_seat_that_sits_in_it() {
    let plan = tmux::layout("desk", "/ws", None, &three());
    let titles: Vec<String> = plan
        .iter()
        .filter(|argv| argv.iter().any(|arg| arg == "@seat"))
        .filter_map(|argv| argv.last().cloned())
        .collect();
    assert_eq!(titles, vec!["@theory", "@solver", "@checker"]);
}

#[test]
fn passes_the_provider_block_through_the_environment_not_the_command_line() {
    let plan = tmux::layout("desk", "/ws", Some(r#"{"provider":"secret"}"#), &three());
    let opener = &plan[0];
    let flag = opener.iter().position(|arg| arg == "-e").expect("no -e");
    assert!(opener[flag + 1].starts_with("OPENCODE_CONFIG_CONTENT="));
    let command = opener.last().expect("no command");
    assert!(
        !command.contains("secret"),
        "the config leaked into the command line: {command}"
    );
}

#[test]
fn a_turn_is_over_only_once_the_session_has_actually_started() {
    let idle = watch(&[serde_json::json!({
        "type": "session.idle",
        "properties": { "sessionID": "ses_1" }
    })]);
    assert!(
        !idle.done(),
        "an idle before the first token would end every turn instantly"
    );
    let both = watch(&[
        serde_json::json!({
            "type": "session.status",
            "properties": { "sessionID": "ses_1", "status": { "type": "busy" } }
        }),
        serde_json::json!({
            "type": "session.idle",
            "properties": { "sessionID": "ses_1" }
        }),
    ]);
    assert!(both.done());
    assert_eq!(both.session(), Some("ses_1"));
}

#[test]
fn does_not_post_the_seats_own_prompt_back_to_the_room() {
    let folded = watch(&[
        serde_json::json!({
            "type": "message.updated",
            "properties": {
                "sessionID": "ses_1",
                "info": { "id": "msg_prompt", "role": "user" }
            }
        }),
        serde_json::json!({
            "type": "message.part.updated",
            "properties": {
                "sessionID": "ses_1",
                "part": {
                    "id": "prt_prompt", "messageID": "msg_prompt",
                    "type": "text", "text": "## This turn\nyou are @theory"
                }
            }
        }),
        serde_json::json!({
            "type": "message.part.updated",
            "properties": {
                "sessionID": "ses_1",
                "part": {
                    "id": "prt_said", "messageID": "msg_reply",
                    "type": "text", "text": "the factor count is k+1"
                }
            }
        }),
    ])
    .finish();
    assert_eq!(folded.message, "the factor count is k+1");
}

#[test]
fn a_part_republished_as_it_runs_is_still_one_tool_call() {
    let folded = watch(&[
        serde_json::json!({
            "type": "message.part.updated",
            "properties": { "sessionID": "ses_1", "part": {
                "id": "prt_a", "messageID": "msg_1", "type": "tool", "tool": "bash",
                "state": { "status": "pending", "input": {} }
            }}
        }),
        serde_json::json!({
            "type": "message.part.updated",
            "properties": { "sessionID": "ses_1", "part": {
                "id": "prt_a", "messageID": "msg_1", "type": "tool", "tool": "bash",
                "state": { "status": "completed", "input": { "command": "python3 factors.py" },
                           "output": "20302" }
            }}
        }),
        serde_json::json!({
            "type": "message.part.updated",
            "properties": { "sessionID": "ses_1", "part": {
                "id": "prt_b", "messageID": "msg_1", "type": "tool", "tool": "read",
                "state": { "status": "completed", "input": { "filePath": "NOTES.md" } }
            }}
        }),
    ])
    .finish();
    assert_eq!(folded.tools, vec!["bash", "read"]);
    assert_eq!(folded.reads, 1);
    assert!(folded.work_log.contains("python3 factors.py"));
    assert!(folded.work_log.contains("20302"));
}

#[test]
fn counts_the_files_the_server_watched_the_seat_write() {
    let folded = watch(&[
        serde_json::json!({
            "type": "file.edited",
            "properties": { "sessionID": "ses_1", "file": "/ws/factors.py" }
        }),
        serde_json::json!({
            "type": "file.edited",
            "properties": { "sessionID": "ses_1", "file": "/ws/factors.py" }
        }),
        serde_json::json!({
            "type": "file.edited",
            "properties": { "sessionID": "ses_1", "file": "/ws/NOTES.md" }
        }),
    ])
    .finish();
    assert_eq!(folded.files_written, vec!["/ws/factors.py", "/ws/NOTES.md"]);
}

#[test]
fn keeps_the_largest_step_total_as_the_turns_tokens() {
    let folded = watch(&[
        serde_json::json!({
            "type": "message.part.updated",
            "properties": { "sessionID": "ses_1", "part": {
                "id": "prt_s1", "messageID": "msg_1", "type": "step-finish",
                "tokens": { "total": 9_000 }
            }}
        }),
        serde_json::json!({
            "type": "message.part.updated",
            "properties": { "sessionID": "ses_1", "part": {
                "id": "prt_s2", "messageID": "msg_1", "type": "step-finish",
                "tokens": { "total": 41_000 }
            }}
        }),
    ])
    .finish();
    assert_eq!(folded.tokens, 41_000);
}

#[test]
fn a_reported_failure_ends_the_turn_rather_than_waiting_out_the_deadline() {
    let seen = watch(&[
        serde_json::json!({
            "type": "session.error",
            "properties": { "sessionID": "ses_1", "error": { "data": {
                "message": "no rung of ladder max-reasoning could serve the request"
            }}}
        }),
        serde_json::json!({
            "type": "session.idle",
            "properties": { "sessionID": "ses_1" }
        }),
    ]);
    assert!(seen.done());
    assert_eq!(
        seen.finish().error.as_deref(),
        Some("no rung of ladder max-reasoning could serve the request")
    );
}

#[test]
fn events_for_another_session_are_not_this_turns_business() {
    let folded = watch(&[
        serde_json::json!({
            "type": "message.part.updated",
            "properties": { "sessionID": "ses_1", "part": {
                "id": "prt_a", "messageID": "msg_1", "type": "text", "text": "mine"
            }}
        }),
        serde_json::json!({
            "type": "message.part.updated",
            "properties": { "sessionID": "ses_other", "part": {
                "id": "prt_b", "messageID": "msg_2", "type": "text", "text": "somebody else"
            }}
        }),
    ])
    .finish();
    assert_eq!(folded.message, "mine");
}

#[test]
fn a_frame_still_being_written_is_left_for_the_next_read() {
    let dir = std::env::temp_dir().join(format!("desk-pane-tail-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("events.jsonl");
    std::fs::write(&path, "data: {\"type\":\"a\"}\ndata: {\"type\"").expect("write");
    let (whole, at) = tail(&path, 0);
    assert_eq!(whole, "data: {\"type\":\"a\"}\n");
    std::fs::write(&path, "data: {\"type\":\"a\"}\ndata: {\"type\":\"b\"}\n").expect("rewrite");
    let (rest, _) = tail(&path, at);
    assert_eq!(rest, "data: {\"type\":\"b\"}\n");
    std::fs::remove_dir_all(&dir).ok();
}
