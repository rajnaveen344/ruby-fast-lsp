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
