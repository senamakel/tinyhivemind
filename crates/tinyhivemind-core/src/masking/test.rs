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

    // With the opener disqualified, the over-indented line is masked as an
    // indented code block of one line instead (see
    // `an_indented_code_block_masks_all_its_lines`), and the un-indented
    // `@alice` line ends that run immediately. The bare closing fence on the
    // last line is then an opener in its own right, unclosed, so it masks to
    // the end of the body.
    let over_indented = "    ```\n@alice\n```\n";
    assert_eq!(
        fenced_ranges(over_indented),
        vec![(0, 8), (15, over_indented.len())]
    );
}

#[test]
fn an_indented_code_block_masks_all_its_lines() {
    // Four or more spaces of indentation is CommonMark's *other* code block,
    // distinct from a backtick or tilde fence. A quoted example written this
    // way — including one that itself contains a bare backtick fence — must
    // stay masked in full, or a directive quoted for documentation would
    // reach a line-leading grammar as if it were live.
    let body = "    ```\n    !unpin ^1\n    ```\n";
    assert_eq!(fenced_ranges(body), vec![(0, body.len())]);
}

#[test]
fn an_indented_code_block_continues_across_a_blank_line() {
    // CommonMark's indented code block does not end at a blank line; only a
    // later non-blank, under-indented line (or the end of the body) closes
    // it.
    let body = "    line one\n\n    line two\nnot indented\n";
    let end_of_block = "    line one\n\n    line two\n".len();
    assert_eq!(fenced_ranges(body), vec![(0, end_of_block)]);
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
fn an_inline_opener_before_a_fenced_block_does_not_pair_across_it() {
    // A stray backtick that opens before a fenced block must not reach past
    // the block to grab a closing backtick after it: the fenced block ends
    // the paragraph the opener lives in, so the opener is unclosed and
    // `@alice` after the block stays live text.
    let body = "`text\n```\ncode\n```\n@alice`";
    assert_eq!(code_ranges(body), vec![(6, 19)]);
}

#[test]
fn a_tab_indented_code_block_masks_all_its_lines() {
    // CommonMark expands a tab to the next multiple of four columns, so one
    // leading tab is already enough indentation to open the block.
    let body = "\t```\n\t!pin ^1\n\t```\n";
    assert_eq!(fenced_ranges(body), vec![(0, body.len())]);
}

#[test]
fn an_indented_line_does_not_open_a_block_mid_paragraph() {
    // An indented code block cannot interrupt a paragraph: a wrapped,
    // indented continuation line right after live text is lazy continuation
    // of that paragraph, not code, so `@alice` here must stay live.
    let body = "some prose\n    @alice\n";
    assert_eq!(fenced_ranges(body), Vec::new());
}

#[test]
fn an_indented_line_opens_a_block_after_a_blank_line() {
    // The same indentation *does* open a block once a blank line separates
    // it from the preceding paragraph.
    let body = "some prose\n\n    @alice\n";
    let block_start = "some prose\n\n".len();
    assert_eq!(fenced_ranges(body), vec![(block_start, body.len())]);
}

#[test]
fn a_masked_range_covers_its_start_but_not_its_end() {
    let ranges = [(2, 5)];
    assert!(!is_masked(1, &ranges));
    assert!(is_masked(2, &ranges));
    assert!(is_masked(4, &ranges));
    assert!(!is_masked(5, &ranges));
}

#[test]
fn an_inline_opener_does_not_pair_across_a_blank_line() {
    // A code span lives inside one paragraph. A blank line ends that
    // paragraph, so a stray backtick above it cannot reach a backtick below
    // it — pairing across the gap would swallow every mention between them.
    let body = "costs 5` a seat\n\nheads up @alice\n\nand a ` tick";
    assert_eq!(code_ranges(body), Vec::new());
    // The same, with nothing but the ticks: two unmatched literal ticks in
    // two paragraphs, not one span across the gap.
    assert_eq!(code_ranges("`\n\n@alice `"), Vec::new());
}

#[test]
fn an_inline_opener_does_not_pair_across_a_line_that_ends_its_paragraph() {
    // The same reasoning as a blank line, for the other single-line blocks
    // this scanner can recognise: an ATX heading, a thematic break, and a
    // setext underline each close the paragraph an opener lives in.
    assert_eq!(code_ranges("a ` b\n# head\n@alice ` c"), Vec::new());
    assert_eq!(code_ranges("a ` b\n***\n@alice ` c"), Vec::new());
    assert_eq!(code_ranges("a ` b\nFoo\n---\n@alice ` c"), Vec::new());
}

#[test]
fn an_escaped_backtick_does_not_open_an_inline_span() {
    // `\`` is a literal backtick, so this body has no code span at all and
    // the mention inside it is live text — which is what a renderer shows.
    assert_eq!(code_ranges(r"\`@alice`"), Vec::new());
    // An escaped backslash is not an escape for what follows it, so the
    // backtick after it does open a span.
    assert_eq!(code_ranges(r"\\`@alice`"), vec![(2, 10)]);
}

#[test]
fn a_backslash_does_not_escape_a_closing_backtick() {
    // Backslash escapes do not apply inside a code span: `` `a\` `` is a
    // span holding `a\`, so the text after it — including the mention — is
    // live.
    assert_eq!(code_ranges(r"`a\`b` @alice"), vec![(0, 4)]);
}

#[test]
fn a_non_breaking_space_line_is_not_a_commonmark_blank_line() {
    // `CommonMark` blank lines hold only spaces and tabs; a non-breaking
    // space is Unicode whitespace but not one of those two bytes, so a line
    // holding only one does not separate paragraphs. A directive after it
    // must not be treated as opening a fresh indented block.
    let body = "prose\n\u{a0}\n    !pin ^1\n";
    assert_eq!(fenced_ranges(body), Vec::new());
}

#[test]
fn an_indented_block_opens_after_a_heading() {
    // An indented block cannot interrupt a *paragraph*, but a heading is not
    // one: it closes above the indented line, so the quoted directive below
    // it is code, exactly as a renderer shows it.
    let body = "# How to pin\n    !pin ^1\n";
    let block = "# How to pin\n".len();
    assert_eq!(fenced_ranges(body), vec![(block, body.len())]);
}

#[test]
fn an_indented_block_opens_after_a_thematic_break_or_a_setext_underline() {
    let broken = "***\n    !pin ^1\n";
    assert_eq!(fenced_ranges(broken), vec![("***\n".len(), broken.len())]);

    let underlined = "Recipes\n---\n    !pin ^1\n";
    assert_eq!(
        fenced_ranges(underlined),
        vec![("Recipes\n---\n".len(), underlined.len())]
    );
}

#[test]
fn an_indented_block_opens_after_a_fenced_block() {
    // A fenced block is not a paragraph either, so the indented line after
    // its closing fence opens a block of its own rather than staying live.
    let body = "```\nx\n```\n    !pin ^1\n";
    let after_fence = "```\nx\n```\n".len();
    assert_eq!(
        fenced_ranges(body),
        vec![(0, after_fence), (after_fence, body.len())]
    );
}

#[test]
fn a_line_that_only_looks_like_a_heading_or_a_break_leaves_the_paragraph_open() {
    // `#hashtag` has no space after its hashes, `===` with nothing above it
    // underlines nothing, `**` is one marker short of a thematic break, and
    // a list marker is not a break at all. Each following indented line is
    // lazy continuation of a live paragraph, so nothing here is masked.
    assert_eq!(fenced_ranges("#hashtag\n    @alice\n"), Vec::new());
    assert_eq!(fenced_ranges("===\n    @alice\n"), Vec::new());
    assert_eq!(fenced_ranges("**\n    @alice\n"), Vec::new());
    assert_eq!(fenced_ranges("- item\n    @alice\n"), Vec::new());
}

#[test]
fn a_range_edge_never_falls_inside_a_character_or_a_line_leading_marker() {
    // Two invariants every consumer leans on. A range edge must land on a
    // character boundary, or a host slicing the body panics; and the start
    // of a line must be masked exactly when the first non-whitespace byte of
    // that line is, because the line-leading grammars test the line start
    // while the mention grammar tests the marker's own offset.
    let bodies = [
        "日本語 `@alice` @bob",
        "héllo `@alice` ok",
        "```\n日本語 @alice\n```\n@bob",
        "    日本語 @alice\n",
        "é`@alice`",
        "# heading\n    !pin ^1\n",
        "```\nx\n```\n    !pin ^1\n",
        "run `\n!pin ^1\n` ok\n!pin ^2",
        "\t!pin ^1\n",
        "prose\n\n   \t!pin ^1\n",
    ];
    for body in bodies {
        let masked = code_ranges(body);
        for (start, end) in &masked {
            assert!(body.is_char_boundary(*start), "{body:?} start {start}");
            assert!(body.is_char_boundary(*end), "{body:?} end {end}");
        }
        let mut line_start = 0;
        for line in body.split_inclusive('\n') {
            let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
            assert_eq!(
                is_masked(line_start, &masked),
                is_masked(line_start + indent, &masked),
                "{body:?} line at {line_start}"
            );
            line_start += line.len();
        }
    }
}

#[test]
fn a_tab_reaches_the_fourth_column_from_any_of_the_first_four() {
    // One tab opens an indented block on its own, and so does any run of
    // one to three spaces followed by a tab, because the tab advances to the
    // next multiple of four. Three spaces without one does not.
    for body in ["\t@alice\n", " \t@alice\n", "  \t@alice\n", "   \t@alice\n"] {
        assert_eq!(fenced_ranges(body), vec![(0, body.len())], "{body:?}");
    }
    assert_eq!(fenced_ranges("   @alice\n"), Vec::new());
}

#[test]
fn a_crlf_body_is_masked_the_same_as_a_lf_one() {
    // A carriage return is part of the line ending, not content, so it
    // neither disqualifies a closing fence nor hides a blank line.
    let fenced = "```rust\r\n@alice\r\n```\r\n";
    assert_eq!(super::fenced_ranges(fenced), vec![(0, fenced.len())]);

    let indented = "prose\r\n\r\n    !pin ^1\r\n";
    let block = "prose\r\n\r\n".len();
    assert_eq!(fenced_ranges(indented), vec![(block, indented.len())]);

    assert_eq!(code_ranges("`@alice`\r\n"), vec![(0, 8)]);
}

#[test]
fn an_empty_body_and_a_bare_opening_fence() {
    assert_eq!(code_ranges(""), Vec::new());
    assert_eq!(fenced_ranges(""), Vec::new());
    // An opener with nothing after it still masks itself, so a body that
    // ends mid-block never leaves its tail readable as grammar.
    assert_eq!(fenced_ranges("```"), vec![(0, 3)]);
    assert_eq!(fenced_ranges("```\n"), vec![(0, 4)]);
    assert_eq!(fenced_ranges("~~~"), vec![(0, 3)]);
}

#[test]
fn a_mask_ends_where_the_line_after_the_closing_fence_begins() {
    // The half-open edge, checked from both sides: the last byte of the
    // block is masked and the first byte after it is not.
    let body = "```\nhidden @alice\n```\n@bob";
    let after = "```\nhidden @alice\n```\n".len();
    assert_eq!(fenced_ranges(body), vec![(0, after)]);
    assert!(is_masked(after - 1, &fenced_ranges(body)));
    assert!(!is_masked(after, &fenced_ranges(body)));
}

#[test]
fn two_adjacent_fenced_blocks_leave_the_line_between_them_live() {
    let body = "```\na\n```\n@alice\n```\nb\n```\n";
    let first = "```\na\n```\n".len();
    let second = "```\na\n```\n@alice\n".len();
    assert_eq!(fenced_ranges(body), vec![(0, first), (second, body.len())]);
    assert!(!is_masked(first, &fenced_ranges(body)));
}

#[test]
fn an_indented_line_inside_a_fenced_block_does_not_extend_past_it() {
    // The indented scan and the fence scan agree about where the block
    // ends: an indented line inside a fence is already masked by the fence,
    // and must not start a run that swallows live text below the close.
    let body = "```\n    @alice\n```\nlive @alice\n";
    let close = "```\n    @alice\n```\n".len();
    assert_eq!(fenced_ranges(body), vec![(0, close)]);
    assert!(!is_masked(close, &fenced_ranges(body)));
}

#[test]
fn a_short_tilde_run_does_not_close_a_longer_tilde_opener() {
    let body = "~~~~\n!pin ^1\n~~~\nstill inside\n~~~~\n";
    assert_eq!(fenced_ranges(body), vec![(0, body.len())]);
}

#[test]
fn an_indented_block_ends_at_the_last_indented_line_not_at_a_trailing_blank() {
    let body = "    code\n\nlive @alice\n";
    assert_eq!(fenced_ranges(body), vec![(0, "    code\n".len())]);
}

#[test]
fn an_indented_line_under_a_list_item_is_masked_though_a_renderer_would_not() {
    // A deliberate, documented divergence: `CommonMark` measures an indented
    // block's four columns inside its container, so this line is a list
    // item's paragraph rather than code. This scanner measures from the left
    // margin and masks it, which costs a directive written that way. Pinned
    // here so the trade is visible rather than assumed.
    let body = "- item\n\n    !pin ^1\n";
    let indented = "- item\n\n".len();
    assert_eq!(fenced_ranges(body), vec![(indented, body.len())]);
}
