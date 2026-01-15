//! Shared utility functions for LSP capabilities

use tower_lsp::lsp_types::Position;

/// Convert LSP Position to byte offset in content
pub fn position_to_offset(content: &str, position: Position) -> usize {
    let mut offset = 0;
    for (line_num, line) in content.lines().enumerate() {
        if line_num == position.line as usize {
            offset += position.character as usize;
            break;
        }
        offset += line.len() + 1;
    }
    offset
}

/// Convert byte offset to 0-indexed line number
pub fn offset_to_line(content: &str, offset: usize) -> u32 {
    let mut current_offset = 0;
    for (line_num, line) in content.lines().enumerate() {
        let line_end = current_offset + line.len();
        if offset <= line_end {
            return line_num as u32;
        }
        current_offset = line_end + 1; // +1 for newline
    }
    // If offset is beyond content, return last line
    content.lines().count().saturating_sub(1) as u32
}
