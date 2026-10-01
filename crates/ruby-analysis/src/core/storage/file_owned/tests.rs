use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Row {
    file: SourceFileId,
    value: u32,
}

impl FileRow for Row {
    fn file_id(&self) -> SourceFileId {
        self.file
    }
}

fn row(file: u32, value: u32) -> Row {
    Row {
        file: SourceFileId(file),
        value,
    }
}

fn by_value(left: &Row, right: &Row) -> Ordering {
    left.value.cmp(&right.value)
}

#[test]
fn replace_orders_rows_and_touches_only_the_target_file() {
    let mut owned = FileOwned::default();
    owned.replace(SourceFileId(1), [row(1, 3), row(1, 1)], by_value);
    owned.replace(SourceFileId(2), [row(2, 7)], by_value);

    let previous = owned.replace(SourceFileId(1), [row(1, 9), row(1, 4)], by_value);

    assert_eq!(previous, Some(vec![row(1, 1), row(1, 3)]));
    assert_eq!(owned.rows(SourceFileId(1)), [row(1, 4), row(1, 9)]);
    assert_eq!(owned.rows(SourceFileId(2)), [row(2, 7)]);
    assert_eq!(owned.len(), 3);
    let mut files = owned.files().collect::<Vec<_>>();
    files.sort();
    assert_eq!(files, [SourceFileId(1), SourceFileId(2)]);
}

#[test]
fn replace_sort_is_stable_for_equal_keys() {
    let mut owned = FileOwned::default();
    owned.replace(
        SourceFileId(1),
        [row(1, 2), row(1, 1), row(1, 2)].map(|mut row| {
            row.value *= 10;
            row
        }),
        |left, right| (left.value / 100).cmp(&(right.value / 100)),
    );

    assert_eq!(
        owned.rows(SourceFileId(1)),
        [row(1, 20), row(1, 10), row(1, 20)]
    );
}

#[test]
fn empty_replacement_leaves_no_entry() {
    let mut owned = FileOwned::default();
    owned.replace(SourceFileId(1), [row(1, 1)], by_value);

    assert_eq!(
        owned.replace(SourceFileId(1), [], by_value),
        Some(vec![row(1, 1)])
    );
    assert_eq!(owned.replace(SourceFileId(3), [], by_value), None);

    assert!(owned.rows(SourceFileId(1)).is_empty());
    assert_eq!(owned.iter().count(), 0);
    assert_eq!(
        owned.estimated_heap_bytes(|_| 0),
        map_table_bytes(&owned.files)
    );
}

#[test]
#[should_panic(expected = "a replacement row belongs to another file")]
fn replace_rejects_a_row_from_another_file() {
    let mut owned = FileOwned::default();
    owned.replace(SourceFileId(1), [row(2, 1)], by_value);
}

mod arena {
    use super::{by_value, row, Row};
    use crate::core::storage::file_owned::arena::{FileArena, FileIndex};
    use crate::core::SourceFileId;

    /// A store with one secondary index keyed by `value % 10`.
    #[derive(Default)]
    struct Indexed {
        rows: FileArena<Row>,
        by_digit: FileIndex<u32>,
    }

    impl Indexed {
        fn replace(&mut self, file: u32, rows: &[Row]) {
            let file_id = SourceFileId(file);
            self.rows.remove_file(file_id, |arena, stale| {
                self.by_digit.unlink(stale.value % 10, file_id, arena)
            });
            let ids = self
                .rows
                .insert_file(file_id, rows.iter().copied(), by_value);
            self.by_digit.link(
                file_id,
                ids.iter().map(|id| (self.rows.get(*id).value % 10, *id)),
                &self.rows,
                by_value,
            );
        }

        fn with_digit(&self, digit: u32) -> Vec<Row> {
            self.by_digit
                .get(&digit)
                .iter()
                .map(|id| *self.rows.get(*id))
                .collect()
        }
    }

    #[test]
    fn index_groups_files_in_id_order_whatever_the_replacement_order() {
        let mut store = Indexed::default();
        store.replace(3, &[row(3, 21), row(3, 1)]);
        store.replace(1, &[row(1, 11)]);
        store.replace(2, &[row(2, 31), row(2, 2)]);

        assert_eq!(
            store.with_digit(1),
            [row(1, 11), row(2, 31), row(3, 1), row(3, 21)]
        );
        assert_eq!(store.with_digit(2), [row(2, 2)]);
        assert_eq!(
            store
                .rows
                .rows_in_file(SourceFileId(2))
                .copied()
                .collect::<Vec<_>>(),
            [row(2, 2), row(2, 31)]
        );
        assert_eq!(store.rows.len(), 5);
    }

    #[test]
    fn replacement_drops_stale_ids_and_reuses_their_slots() {
        let mut store = Indexed::default();
        store.replace(1, &[row(1, 1), row(1, 2)]);
        store.replace(2, &[row(2, 11)]);

        store.replace(1, &[row(1, 41)]);

        assert_eq!(store.with_digit(1), [row(1, 41), row(2, 11)]);
        assert!(store.with_digit(2).is_empty());
        assert_eq!(store.rows.len(), 2);
        assert_eq!(
            store.rows.iter().copied().collect::<Vec<_>>(),
            [row(1, 41), row(2, 11)],
            "a freed slot is reused before the arena grows"
        );

        store.replace(1, &[]);
        assert_eq!(store.with_digit(1), [row(2, 11)]);
        assert!(store.rows.ids_in_file(SourceFileId(1)).is_empty());
        assert_eq!(store.rows.len(), 1);
    }

    #[test]
    #[should_panic(expected = "a file's rows were inserted while its old rows remain")]
    fn insert_requires_the_file_to_be_removed_first() {
        let mut arena = FileArena::default();
        arena.insert_file(SourceFileId(1), [row(1, 1)], by_value);
        arena.insert_file(SourceFileId(1), [row(1, 2)], by_value);
    }

    #[test]
    #[should_panic(expected = "an index still holds ids of the file being linked")]
    fn link_rejects_a_file_whose_stale_ids_were_not_unlinked() {
        let mut arena = FileArena::default();
        let mut index = FileIndex::default();
        let first = arena.insert_file(SourceFileId(1), [row(1, 1)], by_value);
        index.link(SourceFileId(1), [(0_u32, first[0])], &arena, by_value);

        index.link(SourceFileId(1), [(0_u32, first[0])], &arena, by_value);
    }
}
