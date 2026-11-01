use crate::{frontend::SourceRange, Span};
use std::cell::RefCell;
use std::collections::HashMap;

type SourceKey = (usize, usize);
type ActiveIndexes = Vec<HashMap<SourceKey, Vec<usize>>>;
const MIN_INDEXED_SOURCE_BYTES: usize = 1_024;

thread_local! {
    static ACTIVE_INDEXES: RefCell<ActiveIndexes> = const { RefCell::new(Vec::new()) };
}

struct IndexScope {
    previous_depth: usize,
}

impl Drop for IndexScope {
    fn drop(&mut self) {
        ACTIVE_INDEXES.with(|active| active.borrow_mut().truncate(self.previous_depth));
    }
}

/// Run source analysis with newline offsets precomputed for each source.
///
/// The caller must keep each indexed source alive and immutable throughout
/// `run`. The index is dropped with the scope, so pointer reuse between
/// independent analyses cannot return stale locations.
pub(crate) fn with_indexed_sources<'a, R>(
    sources: impl IntoIterator<Item = &'a str>,
    run: impl FnOnce() -> R,
) -> R {
    let mut indexes = HashMap::new();
    for source in sources {
        if source.len() < MIN_INDEXED_SOURCE_BYTES {
            continue;
        }
        let key = (source.as_ptr() as usize, source.len());
        indexes.entry(key).or_insert_with(|| {
            let mut starts =
                Vec::with_capacity(source.bytes().filter(|byte| *byte == b'\n').count() + 1);
            starts.push(0);
            starts.extend(source.match_indices('\n').map(|(offset, _)| offset + 1));
            starts
        });
    }
    if indexes.is_empty() {
        return run();
    }
    let previous_depth = ACTIVE_INDEXES.with(|active| {
        let mut active = active.borrow_mut();
        let previous_depth = active.len();
        active.push(indexes);
        previous_depth
    });
    let scope = IndexScope { previous_depth };
    let result = run();
    drop(scope);
    result
}

pub(crate) fn span(file: &str, source: &str, range: SourceRange) -> Span {
    fn floor_boundary(source: &str, mut offset: usize) -> usize {
        offset = offset.min(source.len());
        while !source.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    }

    fn ceil_boundary(source: &str, mut offset: usize) -> usize {
        offset = offset.min(source.len());
        while !source.is_char_boundary(offset) {
            offset += 1;
        }
        offset
    }

    fn position(source: &str, offset: usize) -> (usize, usize) {
        let prefix = &source[..offset];
        let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
        let column = prefix.rsplit('\n').next().unwrap_or_default().len() + 1;
        (line, column)
    }
    fn indexed_positions(
        source: &str,
        start: usize,
        end: usize,
    ) -> Option<((usize, usize), (usize, usize))> {
        ACTIVE_INDEXES.with(|active| {
            let active = active.borrow();
            let key = (source.as_ptr() as usize, source.len());
            let starts = active.iter().rev().find_map(|indexes| indexes.get(&key))?;
            let position = |offset: usize| {
                let line_index = starts.partition_point(|line_start| *line_start <= offset) - 1;
                (line_index + 1, offset - starts[line_index] + 1)
            };
            Some((position(start), position(end)))
        })
    }
    let start = floor_boundary(source, range.start);
    let end = ceil_boundary(source, range.end).max(start);
    let ((line, column), (end_line, end_column)) = indexed_positions(source, start, end)
        .unwrap_or_else(|| (position(source, start), position(source, end)));
    Span {
        file: file.to_owned(),
        line,
        column,
        end_line,
        end_column,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexed_spans_match_scanned_spans_across_nested_source_scopes() {
        let source = format!("one\ntwö\n{}", "x".repeat(MIN_INDEXED_SOURCE_BYTES));
        let nested = format!("x\ny\nz{}", "x".repeat(MIN_INDEXED_SOURCE_BYTES));
        let source = source.as_str();
        let nested = nested.as_str();
        let range = SourceRange { start: 4, end: 7 };
        let scanned = span("x.py", source, range);
        assert_eq!(scanned.line, 2);
        assert_eq!(scanned.column, 1);
        assert_eq!(scanned.end_line, 2);
        assert_eq!(scanned.end_column, 5);

        with_indexed_sources([source], || {
            assert_eq!(span("x.py", source, range), scanned);
            with_indexed_sources([nested], || {
                assert_eq!(
                    span("y.py", nested, SourceRange { start: 4, end: 5 }).line,
                    3
                );
            });
            assert_eq!(span("x.py", source, range), scanned);
        });
    }
}
