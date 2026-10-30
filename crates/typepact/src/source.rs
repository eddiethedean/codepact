use crate::{frontend::SourceRange, Span};

pub(crate) fn span(file: &str, source: &str, range: SourceRange) -> Span {
    fn position(source: &str, offset: usize) -> (usize, usize) {
        let offset = offset.min(source.len());
        let prefix = &source[..offset];
        let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
        let column = prefix.rsplit('\n').next().unwrap_or_default().len() + 1;
        (line, column)
    }
    let start = range.start.min(source.len());
    let end = range.end.min(source.len()).max(start);
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
