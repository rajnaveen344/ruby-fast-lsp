use lsp_types::Position;
use tree_sitter::Point;

/// Convert an LSP Position to a tree-sitter Point
pub fn position_to_point(position: Position) -> Point {
    Point {
        row: position.line as usize,
        column: position.character as usize,
    }
}

/// Get all starting byte positions of lines in a text buffer
pub fn get_line_starts(text: &[u8]) -> Vec<usize> {
    let mut line_starts = vec![0];
    let mut line_start;

    for (i, &c) in text.iter().enumerate() {
        if c == b'\n' {
            line_start = i + 1;
            line_starts.push(line_start);
        }
    }

    line_starts
}

/// Convert an LSP Position to a byte offset using line starts
pub fn position_to_offset(position: Position, line_starts: &[usize]) -> Option<usize> {
    let line = position.line as usize;
    let col = position.character as usize;

    line_starts.get(line).map(|&line_start| line_start + col)
}

/// Convert a byte offset to an LSP Position
pub fn offset_to_position(offset: usize, line_starts: &[usize]) -> Position {
    // Find which line the offset is on
    let mut line = 0;
    for (i, &start) in line_starts.iter().enumerate() {
        if start > offset {
            break;
        }
        line = i;
    }

    // Character position is the difference between offset and line start
    let character = offset - line_starts[line];

    Position {
        line: line as u32,
        character: character as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_line_starts() {
        let text = b"line1\nline2\nline3";
        let line_starts = get_line_starts(text);
        assert_eq!(line_starts, vec![0, 6, 12]);
    }

    #[test]
    fn test_position_to_offset() {
        let text = b"line1\nline2\nline3";
        let line_starts = get_line_starts(text);

        assert_eq!(
            position_to_offset(Position::new(0, 0), &line_starts),
            Some(0)
        );
        assert_eq!(
            position_to_offset(Position::new(0, 3), &line_starts),
            Some(3)
        );
        assert_eq!(
            position_to_offset(Position::new(1, 2), &line_starts),
            Some(8)
        );
        assert_eq!(
            position_to_offset(Position::new(2, 4), &line_starts),
            Some(16)
        );
    }

    #[test]
    fn test_offset_to_position() {
        let text = b"line1\nline2\nline3";
        let line_starts = get_line_starts(text);

        assert_eq!(offset_to_position(0, &line_starts), Position::new(0, 0));
        assert_eq!(offset_to_position(3, &line_starts), Position::new(0, 3));
        assert_eq!(offset_to_position(8, &line_starts), Position::new(1, 2));
        assert_eq!(offset_to_position(16, &line_starts), Position::new(2, 4));
    }
}
