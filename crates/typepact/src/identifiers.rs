//! Identifier rules generated from the qualified CPython target runtimes.
mod python311;
mod python312;
mod python313;

fn contains(ranges: &[(u32, u32)], character: char) -> bool {
    let point = u32::from(character);
    let index = ranges.partition_point(|(_, end)| *end < point);
    ranges.get(index).is_some_and(|(start, _)| *start <= point)
}

/// Match `str.isidentifier()` for a supported Python source target (3.11–3.13).
/// Other versions use the parser's 3.11 fallback; inspection reports those
/// targets as unsupported independently.
pub fn is_python_identifier(value: &str, python_version: &str) -> bool {
    let (start, continuation) = match python_version {
        "3.12" => (python312::START, python312::CONTINUE_ONLY),
        "3.13" => (python313::START, python313::CONTINUE_ONLY),
        _ => (python311::START, python311::CONTINUE_ONLY),
    };
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|first| contains(start, first))
        && characters
            .all(|character| contains(start, character) || contains(continuation, character))
}
