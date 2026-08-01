use super::SourceFileId;

/// Restore deterministic file/range ordering after one file's facts were
/// appended to an already ordered shared index.
pub(super) fn place_appended_file_facts<T: Copy>(
    ids: &mut [T],
    appended_count: usize,
    file_id: SourceFileId,
    file_id_for: impl Fn(T) -> SourceFileId,
    sort_facts: impl FnOnce(&mut [T]),
) {
    assert!(
        appended_count > 0 && appended_count <= ids.len(),
        "INVARIANT VIOLATED: appended file-fact count is zero or exceeds the shared index length. \
         This is a bug because a store must record only facts it appended to that index. \
         Fix: count the target file's appended facts before restoring index order."
    );
    let appended_start = ids.len() - appended_count;
    sort_facts(&mut ids[appended_start..]);

    let insertion = ids[..appended_start].partition_point(|id| file_id_for(*id) < file_id);
    if let Some(existing) = ids[..appended_start].get(insertion) {
        assert!(
            file_id_for(*existing) != file_id,
            "INVARIANT VIOLATED: a shared fact index still contains the file being replaced. \
             This is a bug because stale file facts must be removed before replacement facts are appended. \
             Fix: remove the target SourceFileId from every store index before reinsertion."
        );
    }
    if insertion != appended_start {
        ids[insertion..].rotate_right(appended_count);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    #[test]
    fn appending_one_later_file_does_not_resort_the_existing_bucket() {
        let mut ids = (0..4096_u32)
            .map(|file| (SourceFileId(file), 0_u32))
            .collect::<Vec<_>>();
        ids.push((SourceFileId(4096), 0));

        let comparisons = Cell::new(0_usize);
        place_appended_file_facts(
            &mut ids,
            1,
            SourceFileId(4096),
            |entry| {
                comparisons.set(comparisons.get() + 1);
                entry.0
            },
            |facts| {
                facts.sort_by(|left, right| {
                    comparisons.set(comparisons.get() + 1);
                    left.cmp(right)
                });
            },
        );

        assert!(
            comparisons.get() <= 32,
            "appending one ordered file must inspect only the appended tail and a logarithmic \
             insertion path, not re-sort {} existing facts ({} comparisons)",
            ids.len() - 1,
            comparisons.get()
        );
    }

    #[test]
    fn inserted_file_group_is_sorted_and_rotated_as_one_stable_unit() {
        let mut ids = vec![
            (SourceFileId(1), 4_u32),
            (SourceFileId(3), 7_u32),
            (SourceFileId(2), 9_u32),
            (SourceFileId(2), 2_u32),
        ];

        place_appended_file_facts(
            &mut ids,
            2,
            SourceFileId(2),
            |entry| entry.0,
            |facts| facts.sort_by_key(|entry| entry.1),
        );

        assert_eq!(
            ids,
            vec![
                (SourceFileId(1), 4),
                (SourceFileId(2), 2),
                (SourceFileId(2), 9),
                (SourceFileId(3), 7),
            ]
        );
    }

    #[test]
    #[should_panic(expected = "a shared fact index still contains the file being replaced")]
    fn rejects_a_file_group_that_was_not_removed_before_replacement() {
        let mut ids = vec![
            (SourceFileId(1), 1_u32),
            (SourceFileId(2), 1_u32),
            (SourceFileId(2), 2_u32),
        ];
        place_appended_file_facts(
            &mut ids,
            1,
            SourceFileId(2),
            |entry| entry.0,
            |facts| facts.sort(),
        );
    }
}
