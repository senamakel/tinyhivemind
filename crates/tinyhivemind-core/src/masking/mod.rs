//! Which byte ranges of a message body are code, and so are not grammar.

#[cfg(test)]
mod test;

/// Byte ranges of `body` covered by a fenced code block.
#[must_use]
pub fn fenced_ranges(body: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut open: Option<(usize, char)> = None;
    let mut offset = 0;
    for line in body.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        let trimmed = line.trim_start();
        let fence = trimmed
            .starts_with("```")
            .then_some('`')
            .or_else(|| trimmed.starts_with("~~~").then_some('~'));
        let Some(fence) = fence else { continue };
        match open {
            None => open = Some((start, fence)),
            Some((from, opener)) if opener == fence => {
                ranges.push((from, offset));
                open = None;
            }
            Some(_) => {}
        }
    }
    if let Some((from, _)) = open {
        ranges.push((from, body.len()));
    }
    ranges
}
