use ruby_analysis::core::TextRange;
use ruby_analysis::engine::{AnalysisEngine, SourceFile};
use tower_lsp::lsp_types::{Location, Position, Range, Url};

pub(crate) fn location_for_range(engine: &AnalysisEngine, range: TextRange) -> Option<Location> {
    let file = engine.file(range.file_id)?;
    Some(Location {
        uri: source_file_uri(file)?,
        range: lsp_range_for_text_range(file, range)?,
    })
}

pub(crate) fn locations_for_ranges(
    engine: &AnalysisEngine,
    ranges: impl IntoIterator<Item = TextRange>,
) -> Vec<Location> {
    ranges
        .into_iter()
        .filter_map(|range| location_for_range(engine, range))
        .collect()
}

pub(crate) fn lsp_ranges_for_ranges(
    engine: &AnalysisEngine,
    ranges: impl IntoIterator<Item = TextRange>,
) -> Vec<Range> {
    ranges
        .into_iter()
        .filter_map(|range| location_for_range(engine, range).map(|location| location.range))
        .collect()
}

pub(crate) fn non_empty_locations(locations: Vec<Location>) -> Option<Vec<Location>> {
    if locations.is_empty() {
        None
    } else {
        Some(locations)
    }
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
    let (start_line, start_character) = file.byte_offset_to_line_character(range.start_byte)?;
    let (end_line, end_character) = file.byte_offset_to_line_character(range.end_byte)?;
    Some(Range::new(
        Position::new(start_line, start_character),
        Position::new(end_line, end_character),
    ))
}
