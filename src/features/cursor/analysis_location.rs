use ruby_analysis::core::TextRange;
use ruby_analysis::engine::{SourceFile, View};
use tower_lsp::lsp_types::{Location, Range, Url};

use crate::utils::lsp::lsp_file_range;

pub(crate) fn location_for_range(view: &View<'_>, range: TextRange) -> Option<Location> {
    let file = view.file(range.file_id)?;
    Some(Location {
        uri: source_file_uri(file)?,
        range: lsp_file_range(file, range)?,
    })
}

pub(crate) fn locations_for_ranges(
    view: &View<'_>,
    ranges: impl IntoIterator<Item = TextRange>,
) -> Vec<Location> {
    ranges
        .into_iter()
        .filter_map(|range| location_for_range(view, range))
        .collect()
}

pub(crate) fn lsp_ranges_for_ranges(
    view: &View<'_>,
    ranges: impl IntoIterator<Item = TextRange>,
) -> Vec<Range> {
    ranges
        .into_iter()
        .filter_map(|range| location_for_range(view, range).map(|location| location.range))
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
