use crate::{frontend::SourceRange, Span};

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
    let start = floor_boundary(source, range.start);
    let end = ceil_boundary(source, range.end).max(start);
    let (line, column) = position(source, start);
    let (end_line, end_column) = position(source, end);
    Span {
        file: file.to_owned(),
        line,
        column,
        end_line,
        end_column,
    }
}
