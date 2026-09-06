//! Which byte ranges of a message body are code, and so carry no grammar.
//!
//! Every authored grammar in this workspace — the mention grammar here, the
//! stigmergic trace grammar in `tinyhivemind-hive`, the pin directives in
//! `tinyhivemind` — reads the *same* message body. If each carried its own
//! scanner they could disagree about which span of a body is code, and a
//! marker one grammar treats as quoted documentation the next would treat as
//! a live instruction. This module is the single scanner they share.
//!
//! # Two levels of masking, and why
//!
//! [`fenced_ranges`] masks fenced code blocks only. [`code_ranges`] masks
//! those *and* inline code spans. A grammar picks the level its markers
//! need:
//!
//! - A **line-leading** grammar — one whose marker only counts at the start of
//!   a line, like `!propose` or a pin directive — needs fences alone. A marker
//!   preceded by an inline backtick is by definition not line-leading, so
//!   inline masking would only cost a scan.
//! - A **mid-line** grammar — a mention, which may appear anywhere in a
//!   sentence — needs both. `` `@alice` `` is a name in prose about a name,
//!   not a ping.
//!
//! # Fence rules
//!
//! The fence rules follow CommonMark closely enough that an author who
//! formats a message for a Markdown renderer gets the masking they see:
//!
//! - an opening fence is indented at most three spaces, and is a run of at
//!   least three backticks or at least three tildes;
//! - a backtick opener's info string may not contain a backtick, because that
//!   is an inline span in a paragraph rather than a block;
//! - a closing fence uses the same character as its opener, runs at least as
//!   long, and carries nothing but whitespace after the run;
//! - an unclosed fence masks to the end of the body, so a body cannot end
//!   mid-block with its tail read as grammar.

#[cfg(test)]
mod test;

/// Every byte range of `body` that is code: fenced blocks and inline spans.
///
/// Ranges are half-open and returned in ascending order. Use this for a
/// grammar whose markers may appear mid-line; a line-leading grammar wants
/// [`fenced_ranges`] instead.
///
/// ```
/// use tinyhivemind_core::masking::code_ranges;
///
/// assert_eq!(code_ranges("`@alice` @bob"), vec![(0, 8)]);
/// ```
#[must_use]
pub fn code_ranges(body: &str) -> Vec<(usize, usize)> {
    fenced_ranges(body)
}

/// Whether the byte at `offset` falls inside one of `ranges`.
///
/// `ranges` are half-open, as [`code_ranges`] and [`fenced_ranges`] return
/// them, so a range's `end` is not masked.
///
/// ```
/// use tinyhivemind_core::masking::{code_ranges, is_masked};
///
/// let body = "`@alice` @bob";
/// let masked = code_ranges(body);
/// assert!(is_masked(1, &masked));
/// assert!(!is_masked(9, &masked));
/// ```
#[must_use]
pub fn is_masked(offset: usize, ranges: &[(usize, usize)]) -> bool {
    ranges
        .iter()
        .any(|(start, end)| *start <= offset && offset < *end)
}

/// The byte ranges of `body` covered by a fenced code block.
///
/// Each range is half-open — `start` is masked, `end` is not — and they are
/// returned in the order the blocks open, which is also ascending order. Use
/// this for a line-leading grammar; see [`code_ranges`] for one that also
/// needs inline spans masked.
///
/// ```
/// use tinyhivemind_core::masking::fenced_ranges;
///
/// // A ```rust line inside an open block is content, not a closing fence.
/// let body = "```\nhidden\n```rust\nalso hidden\n```\n";
/// assert_eq!(fenced_ranges(body), vec![(0, body.len())]);
/// ```
#[must_use]
pub fn fenced_ranges(body: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut open: Option<(usize, u8, usize)> = None;
    let mut line_start = 0;
    for line in body.split_inclusive('\n') {
        let trimmed = line.trim_start_matches(' ');
        let indent = line.len() - trimmed.len();
        if indent <= 3 {
            let marker = trimmed.as_bytes().first().copied();
            if matches!(marker, Some(b'`' | b'~')) {
                let marker = marker.unwrap_or_default();
                let run = trimmed
                    .as_bytes()
                    .iter()
                    .take_while(|byte| **byte == marker)
                    .count();
                if run >= 3 {
                    match open {
                        None if marker == b'~' || !trimmed[run..].contains('`') => {
                            open = Some((line_start, marker, run));
                        }
                        Some((start, open_marker, open_run))
                            if marker == open_marker
                                && run >= open_run
                                && trimmed[run..].trim().is_empty() =>
                        {
                            ranges.push((start, line_start + line.len()));
                            open = None;
                        }
                        None | Some(_) => {}
                    }
                }
            }
        }
        line_start += line.len();
    }
    if let Some((start, _, _)) = open {
        ranges.push((start, body.len()));
    }
    ranges
}
