//! Character-by-character edit scripts derived from a file's own text.

/// At most this many lines per file are erased and retyped.
const MAX_EDITED_LINES: usize = 3;

/// One buffer state on the way through a script, with the cursor that produced it.
pub struct Keystroke {
    pub content: String,
    pub line: u32,
    pub character: u32,
}

/// Lines worth retyping: the longest non-blank lines, spread across the file.
pub fn edited_lines(content: &str) -> Vec<usize> {
    let lines = content.lines().collect::<Vec<_>>();
    let mut candidates = lines
        .iter()
        .enumerate()
        .filter(|(_, text)| !text.trim().is_empty())
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Vec::new();
    }
    let chunk = candidates.len().div_ceil(MAX_EDITED_LINES);
    candidates
        .chunks_mut(chunk)
        .filter_map(|group| {
            group
                .iter()
                .copied()
                .max_by_key(|&index| (lines[index].trim().len(), std::cmp::Reverse(index)))
        })
        .collect()
}

/// Erases `line` one character at a time from its end, then types it back.
/// The final state equals the original content.
pub fn erase_and_retype(content: &str, line: usize) -> Vec<Keystroke> {
    let mut lines = content.split('\n').map(str::to_string).collect::<Vec<_>>();
    let original = lines[line].clone();
    let indent = original.len() - original.trim_start().len();
    let typed = original[indent..].chars().collect::<Vec<_>>();
    let mut steps = Vec::with_capacity(typed.len() * 2);

    for kept in (0..typed.len()).rev() {
        lines[line] = format!("{}{}", &original[..indent], prefix(&typed, kept));
        steps.push(keystroke(&lines, line, &original[..indent], &typed[..kept]));
    }
    for kept in 1..=typed.len() {
        lines[line] = format!("{}{}", &original[..indent], prefix(&typed, kept));
        steps.push(keystroke(&lines, line, &original[..indent], &typed[..kept]));
    }
    assert_eq!(
        steps
            .last()
            .map(|step| step.content.as_str())
            .unwrap_or(content),
        content,
        "an erase-and-retype script must restore the original buffer"
    );
    steps
}

fn prefix(chars: &[char], count: usize) -> String {
    chars[..count].iter().collect()
}

fn keystroke(lines: &[String], line: usize, indent: &str, typed: &[char]) -> Keystroke {
    let character =
        indent.encode_utf16().count() + typed.iter().map(|ch| ch.len_utf16()).sum::<usize>();
    Keystroke {
        content: lines.join("\n"),
        line: line as u32,
        character: character as u32,
    }
}

#[cfg(test)]
mod script_tests {
    use super::*;

    #[test]
    fn retyping_restores_the_buffer_and_tracks_utf16_cursors() {
        let content = "a = 1\n  b = \"📦x\"\nc";
        let steps = erase_and_retype(content, 1);
        assert_eq!(steps.last().unwrap().content, content);
        let empty = steps
            .iter()
            .find(|step| step.content == "a = 1\n  \nc")
            .unwrap();
        assert_eq!((empty.line, empty.character), (1, 2));
        let after_box = steps
            .iter()
            .find(|step| step.content == "a = 1\n  b = \"📦\nc")
            .unwrap();
        assert_eq!(after_box.character, 2 + 5 + 2);
    }

    #[test]
    fn edited_lines_spread_across_the_file() {
        let content = "x\nlonger line\n\nmid\nthe longest line here\nz\nend line long";
        assert_eq!(edited_lines(content), vec![1, 4, 6]);
    }
}
