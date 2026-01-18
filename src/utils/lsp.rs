//! LSP utility functions

use tower_lsp::lsp_types::Location;

/// Remove duplicate locations from a vector.
pub fn deduplicate_locations(locations: Vec<Location>) -> Vec<Location> {
    let mut unique = Vec::new();
    for loc in locations {
        if !unique
            .iter()
            .any(|existing: &Location| existing.uri == loc.uri && existing.range == loc.range)
        {
            unique.push(loc);
        }
    }
    unique
}
