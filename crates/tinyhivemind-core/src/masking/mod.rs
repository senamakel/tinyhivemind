//! Which byte ranges of a message body are code, and so carry no grammar.
//!
//! The mention grammar here and the stigmergic trace grammar in
//! `tinyhivemind-hive` read the *same* message body. While each carried its
//! own scanner they could disagree about which span of that body is code, and
//! a marker one grammar read as quoted documentation the other read as a live
//! instruction. This module is the one scanner they share.
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
//! [`fenced_ranges`] covers both of `CommonMark`'s code block kinds, closely
//! enough that an author who formats a message for a Markdown renderer gets
//! the masking they see:
//!
//! - an opening fence is indented at most three spaces, and is a run of at
//!   least three backticks or at least three tildes;
//! - a backtick opener's info string may not contain a backtick, because that
//!   is an inline span in a paragraph rather than a block;
//! - a closing fence uses the same character as its opener, runs at least as
//!   long, and carries nothing but whitespace after the run;
//! - an unclosed fence masks to the end of the body, so a body cannot end
//!   mid-block with its tail read as grammar;
//! - a line indented four or more columns — a tab expands to the next
//!   multiple of four — opens an [indented code block][indented-spec] instead,
//!   which is why a fence at that indentation never opens a fenced one — but
//!   its content is still `CommonMark` code, so [`fenced_ranges`] masks it
//!   too, and a directive quoted inside one cannot reach a line-leading
//!   grammar;
//! - an indented block cannot interrupt a paragraph: the indented *opener*
//!   must be the first line of the body or follow a blank line, so a wrapped,
//!   indented continuation line right after live text stays live rather than
//!   being read as a new block.
//!
//! [spec]: https://spec.commonmark.org/current/#fenced-code-blocks
//! [indented-spec]: https://spec.commonmark.org/current/#indented-code-blocks

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
    let mut ranges = fenced_ranges(body);
    ranges.extend(inline_ranges(body, &ranges));
    ranges.sort_unstable();
    ranges
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

/// The byte ranges of `body` covered by a fenced or indented code block.
///
/// Each range is half-open — `start` is masked, `end` is not — and they are
/// returned in ascending order. Use this for a line-leading grammar; see
/// [`code_ranges`] for one that also needs inline spans masked.
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
    let mut ranges = backtick_or_tilde_fence_ranges(body);
    ranges.extend(indented_block_ranges(body));
    ranges.sort_unstable();
    ranges
}

fn backtick_or_tilde_fence_ranges(body: &str) -> Vec<(usize, usize)> {
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

/// A maximal run of lines each indented at least four spaces: `CommonMark`'s
/// other code block, distinct from a backtick or tilde fence.
///
/// A blank line does not end the run — only a later non-blank, under-indented
/// line or the end of the body does — because `CommonMark` lets an indented
/// block continue across blank lines. This is a conservative approximation
/// (it does not track list or blockquote context the way a full parser
/// would), but it never under-masks a plainly indented example: a fenced pair
/// quoted at four or more spaces of indentation is `CommonMark` code either
/// way, so treating its content as masked is never a false positive against
/// what a Markdown renderer would show.
fn indented_block_ranges(body: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut open: Option<usize> = None;
    let mut open_end = 0;
    let mut line_start = 0;
    // An indented code block cannot interrupt a paragraph: the *opening*
    // line must follow a blank line, or be the first line of the body.
    // `prev_blank` starts `true` so a block may open at the very start.
    let mut prev_blank = true;
    for line in body.split_inclusive('\n') {
        let content = line.trim_end_matches(['\n', '\r']);
        if content.trim().is_empty() {
            prev_blank = true;
            line_start += line.len();
            continue;
        }
        let indent = indentation_width(content);
        if indent >= 4 && (open.is_some() || prev_blank) {
            open.get_or_insert(line_start);
            open_end = line_start + line.len();
        } else if let Some(start) = open.take() {
            ranges.push((start, open_end));
        }
        prev_blank = false;
        line_start += line.len();
    }
    if let Some(start) = open {
        ranges.push((start, open_end));
    }
    ranges
}

/// `CommonMark`'s indentation width of a line's leading whitespace: a space
/// advances one column and a tab advances to the next multiple of four, the
/// same rule fence detection above does not need because a fence marker is
/// never itself whitespace.
fn indentation_width(content: &str) -> usize {
    let mut width = 0;
    for byte in content.bytes() {
        match byte {
            b' ' => width += 1,
            b'\t' => width += 4 - (width % 4),
            _ => break,
        }
    }
    width
}

fn inline_ranges(body: &str, fenced: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let bytes = body.as_bytes();
    let mut ranges = Vec::new();
    let mut offset = 0;
    while offset < bytes.len() {
        if let Some((_, end)) = fenced
            .iter()
            .find(|(start, end)| *start <= offset && offset < *end)
        {
            offset = *end;
            continue;
        }
        if bytes[offset] != b'`' {
            offset += 1;
            continue;
        }
        let run = bytes[offset..]
            .iter()
            .take_while(|byte| **byte == b'`')
            .count();
        let mut candidate = offset + run;
        let mut closing = None;
        while candidate < bytes.len() {
            // A fenced block ends the paragraph this opener lives in, so the
            // search for a closing run must not cross it: pairing across a
            // fenced block would mask live text after the block as if it
            // were still inside this opener's inline span.
            if is_masked(candidate, fenced) {
                break;
            }
            if bytes[candidate] != b'`' {
                candidate += 1;
                continue;
            }
            let closing_run = bytes[candidate..]
                .iter()
                .take_while(|byte| **byte == b'`')
                .count();
            if closing_run == run {
                closing = Some(candidate + closing_run);
                break;
            }
            candidate += closing_run;
        }
        if let Some(end) = closing {
            ranges.push((offset, end));
            offset = end;
        } else {
            offset += run;
        }
    }
    ranges
}
