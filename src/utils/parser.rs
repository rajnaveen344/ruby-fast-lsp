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
