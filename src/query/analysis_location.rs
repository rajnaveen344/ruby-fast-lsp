use ruby_analysis_core::TextRange;
use ruby_analysis_engine::{AnalysisEngine, SourceFile};
use tower_lsp::lsp_types::{Location, Position, Range, Url};

pub(crate) fn location_for_range(engine: &AnalysisEngine, range: TextRange) -> Option<Location> {
    let file = engine.file(range.file_id)?;
    Some(Location {
        uri: source_file_uri(file)?,
        range: lsp_range_for_text_range(file, range)?,
    })
}

fn source_file_uri(file: &SourceFile) -> Option<Url> {
    Url::from_file_path(&file.path).ok()
}

fn lsp_range_for_text_range(file: &SourceFile, range: TextRange) -> Option<Range> {
    assert!(
        file.id == range.file_id,
        "INVARIANT VIOLATED: analysis range file id does not match source file id. \
         This is a bug because analysis facts must only be converted with their owning source file. \
         Fix: look up the SourceFile by range.file_id before converting."
    );
    Some(Range::new(
        byte_offset_to_position(&file.source, range.start_byte)?,
        byte_offset_to_position(&file.source, range.end_byte)?,
    ))
}

fn byte_offset_to_position(source: &str, byte_offset: u32) -> Option<Position> {
    let target = usize::try_from(byte_offset).ok()?;
    if target > source.len() || !source.is_char_boundary(target) {
        return None;
    }

    let mut line = 0u32;
    let mut line_start = 0usize;
    for (idx, byte) in source.bytes().enumerate() {
        if idx >= target {
            break;
        }
        if byte == b'\n' {
            line += 1;
            line_start = idx + 1;
        }
    }
    let character = source[line_start..target].chars().count();
    let character = u32::try_from(character).expect(
        "INVARIANT VIOLATED: LSP character offset exceeded u32. \
         This is a bug because LSP positions require u32 columns. \
         Fix: reject or segment lines longer than u32::MAX characters.",
    );
    Some(Position::new(line, character))
}
