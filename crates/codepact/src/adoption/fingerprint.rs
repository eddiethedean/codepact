use super::*;
use ruff_python_ast::token::TokenKind;
use ruff_python_ast::PythonVersion;
use ruff_python_parser::{Mode, ParseOptions};
use ruff_text_size::Ranged;
use sha2::{Digest, Sha256};

pub(super) fn hash(data: &[u8]) -> String {
    let d = Sha256::digest(data);
    d.iter().map(|b| format!("{b:02x}")).collect()
}
pub(super) fn code_before_comment(line: &str) -> &str {
    let b = line.as_bytes();
    let (mut i, mut quote, mut triple, mut escaped) = (0usize, None::<u8>, false, false);
    while i < b.len() {
        let c = b[i];
        if let Some(q) = quote {
            if escaped {
                escaped = false;
                i += 1;
                continue;
            }
            if c == b'\\' {
                escaped = true;
                i += 1;
                continue;
            }
            if triple && i + 2 < b.len() && b[i] == q && b[i + 1] == q && b[i + 2] == q {
                quote = None;
                triple = false;
                i += 3;
                continue;
            }
            if !triple && c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        if c == b'#' {
            return &line[..i];
        }
        if c == b'\'' || c == b'"' {
            quote = Some(c);
            triple = i + 2 < b.len() && b[i + 1] == c && b[i + 2] == c;
            if triple {
                i += 3;
            } else {
                i += 1;
            }
            continue;
        }
        i += 1;
    }
    line
}
fn normalized_code(source: &str) -> String {
    let source = source.trim();
    let parsed = ruff_python_parser::parse_unchecked(
        source,
        ParseOptions::from(Mode::Module).with_target_version(PythonVersion::PY313),
    );
    let tokens = parsed
        .tokens()
        .iter()
        .filter_map(|token| {
            let kind = token.kind();
            if matches!(
                kind,
                TokenKind::Comment | TokenKind::NonLogicalNewline | TokenKind::EndOfFile
            ) {
                return None;
            }
            let text = if matches!(
                kind,
                TokenKind::Indent | TokenKind::Dedent | TokenKind::Newline
            ) {
                ""
            } else {
                &source[token.start().to_usize()..token.end().to_usize()]
            };
            Some((format!("{kind:?}"), text))
        })
        .collect::<Vec<_>>();
    serde_json::to_string(&tokens).expect("canonical tokens serialize")
}

fn enclosing_declaration(source: &str, line: usize, starts: &[usize]) -> String {
    let mut enclosing = Vec::<(usize, String)>::new();
    for (index, &start) in starts.iter().enumerate().take(line) {
        let end = starts.get(index + 1).copied().unwrap_or(source.len());
        let candidate = crate::source::line_content(source, start, end);
        let code = code_before_comment(candidate);
        let trimmed = code.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        let indent = code
            .chars()
            .take_while(|character| character.is_whitespace())
            .fold(0usize, |column, character| match character {
                '\t' => column + (8 - column % 8),
                _ => column + 1,
            });
        while enclosing
            .last()
            .is_some_and(|(previous_indent, _)| *previous_indent >= indent)
        {
            enclosing.pop();
        }
        let is_declaration = trimmed.starts_with("def ")
            || trimmed.starts_with("async def ")
            || trimmed.starts_with("class ");
        if is_declaration {
            enclosing.push((indent, normalized_code(trimmed)));
        }
    }
    if enclosing.is_empty() {
        "<module>".into()
    } else {
        enclosing
            .into_iter()
            .map(|(_, declaration)| declaration)
            .collect::<Vec<_>>()
            .join("/")
    }
}

fn normalized_span(source: &str, span: &Span, starts: &[usize]) -> String {
    let offset = |line: usize, column: usize| {
        starts
            .get(line.saturating_sub(1))
            .copied()
            .unwrap_or(source.len())
            .saturating_add(column.saturating_sub(1))
    };
    let start = offset(span.line, span.column);
    let end = offset(span.end_line, span.end_column);
    normalized_code(source.get(start..end).unwrap_or(""))
}

#[derive(Default)]
pub(super) struct AnchorCache {
    anchors: BTreeMap<(String, usize, usize, usize, usize), String>,
    line_starts: BTreeMap<String, Vec<usize>>,
}

pub(super) fn semantic_anchor(source: &str, span: &Span, cache: &mut AnchorCache) -> String {
    let starts = cache
        .line_starts
        .entry(span.file.clone())
        .or_insert_with(|| crate::source::line_starts(source));
    cache
        .anchors
        .entry((
            span.file.clone(),
            span.line,
            span.column,
            span.end_line,
            span.end_column,
        ))
        .or_insert_with(|| {
            hash(
                serde_json::to_string(&(
                    enclosing_declaration(source, span.line, starts),
                    normalized_span(source, span, starts),
                ))
                .expect("semantic anchor serializes")
                .as_bytes(),
            )
        })
        .clone()
}

pub(super) fn source_comments(source: &str) -> Vec<(usize, &str)> {
    let mut comments = Vec::new();
    // Use the same Python lexer family as source analysis. In particular, this
    // keeps quotes inside PEP 701 f-string expressions from ending the outer
    // string and turning string contents into apparent suppression comments.
    let parsed = ruff_python_parser::parse_unchecked(
        source,
        ParseOptions::from(Mode::Module).with_target_version(PythonVersion::PY313),
    );
    let line_starts = crate::source::line_starts(source);
    for token in parsed.tokens() {
        if token.kind() != TokenKind::Comment {
            continue;
        }
        let start = token.start().to_usize();
        let end = token.end().to_usize();
        let line = line_starts.partition_point(|line_start| *line_start <= start);
        comments.push((line, &source[start..end]));
    }
    comments
}

pub(super) fn fingerprint(
    diagnostic: &CheckDiagnostic,
    contents: &BTreeMap<&str, &str>,
    cache: &mut AnchorCache,
) -> String {
    let source = contents
        .get(diagnostic.primary.file.as_str())
        .copied()
        .unwrap_or("");
    let related = diagnostic
        .related
        .iter()
        .map(|span| {
            let related_source = contents.get(span.file.as_str()).copied().unwrap_or("");
            format!(
                "{}:{}",
                span.file,
                semantic_anchor(related_source, span, cache)
            )
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join("|");
    let encoding = [
        format!("fingerprint-v{}", FINGERPRINT_ALGORITHM_VERSION),
        RULE_SEMANTIC_VERSION.to_string(),
        diagnostic.code.clone(),
        diagnostic.primary.file.clone(),
        semantic_anchor(source, &diagnostic.primary, cache),
        diagnostic.certainty.clone(),
        diagnostic.trust.clone(),
        diagnostic.completion.clone().unwrap_or_default(),
        related,
        serde_json::to_string(&diagnostic.semantic_identity).expect("identity serializes"),
    ]
    .join("\0");
    hash(encoding.as_bytes())
}
