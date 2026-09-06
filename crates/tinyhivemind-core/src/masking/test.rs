//! Unit tests for the code masking scanner.

use super::*;

#[test]
fn a_fence_indented_four_spaces_does_not_open_a_block() {
    // CommonMark allows an opening fence up to three spaces of indentation.
    // A fourth space makes the line an indented code block instead, and an
    // indented block never opens a fence that could swallow the lines after
    // it.
    let indented = "   ```\n@alice\n```\n";
    assert_eq!(fenced_ranges(indented), vec![(0, indented.len())]);

    // With the opener disqualified the bare fence on the last line is an
    // opener in its own right, and masks to the end of the body.
    let over_indented = "    ```\n@alice\n```\n";
    assert_eq!(fenced_ranges(over_indented), vec![(15, over_indented.len())]);
}

#[test]
fn a_backtick_opener_carrying_a_backtick_in_its_info_string_is_not_a_fence() {
    // An info string on a backtick fence may not contain a backtick, so
    // ```a`b is an inline span in a paragraph, not the start of a block. The
    // bare fence two lines later is the real opener, and it never closes.
    let body = "```a`b\ncontent\n```\n";
    assert_eq!(fenced_ranges(body), vec![(15, 19)]);
}

#[test]
fn a_tilde_opener_may_carry_a_backtick_in_its_info_string() {
    // The restriction is a backtick-fence rule only: a tilde fence's info
    // string is free-form, so this line does open a block.
    let body = "~~~a`b\ncontent\n~~~\n";
    assert_eq!(fenced_ranges(body), vec![(0, 19)]);
}

#[test]
fn a_closing_fence_must_run_at_least_as_long_as_the_opener() {
    // A four-backtick block quoting a three-backtick example is one block.
    // Closing on the shorter run would resume grammar parsing inside quoted
    // documentation.
    let body = "````\n```\ncontent\n````\n";
    assert_eq!(fenced_ranges(body), vec![(0, 22)]);
}

#[test]
fn a_closing_fence_carries_nothing_after_its_run() {
    // An info string marks an *opening* fence. A ```rust line inside an open
    // block is content, so the block runs on to the bare fence below it.
    let body = "```\nhidden\n```rust\nstill hidden\n```\n";
    assert_eq!(fenced_ranges(body), vec![(0, body.len())]);
}

#[test]
fn a_tilde_fence_does_not_close_a_backtick_fence() {
    assert_eq!(fenced_ranges("```\n~~~\ncontent\n```\n"), vec![(0, 20)]);
    assert_eq!(fenced_ranges("~~~\n```\ncontent\n~~~\n"), vec![(0, 20)]);
}

#[test]
fn an_unterminated_fence_masks_to_the_end_of_the_body() {
    assert_eq!(fenced_ranges("```\n@alice"), vec![(0, 10)]);
    assert_eq!(fenced_ranges("~~~\n@alice"), vec![(0, 10)]);
}

#[test]
fn an_inline_span_is_masked_by_code_ranges_but_not_by_fenced_ranges() {
    // The difference between the two levels, in one body: a line-leading
    // grammar sees no code here at all, a mid-line grammar sees the span.
    let body = "`@alice` @bob";
    assert_eq!(fenced_ranges(body), Vec::new());
    assert_eq!(code_ranges(body), vec![(0, 8)]);
}

#[test]
fn an_unclosed_inline_tick_masks_nothing() {
    // A lone backtick in prose is punctuation, not the start of a span that
    // silences the rest of the message.
    assert_eq!(code_ranges("` text @alice"), Vec::new());
}

#[test]
fn a_backtick_inside_a_fenced_block_does_not_open_an_inline_span() {
    // Otherwise a stray tick inside a block would pair with one after the
    // close and mask live text between them.
    let body = "```\n` code\n```\n@alice `";
    assert_eq!(code_ranges(body), vec![(0, 15)]);
}

#[test]
fn a_masked_range_covers_its_start_but_not_its_end() {
    let ranges = [(2, 5)];
    assert!(!is_masked(1, &ranges));
    assert!(is_masked(2, &ranges));
    assert!(is_masked(4, &ranges));
    assert!(!is_masked(5, &ranges));
}
