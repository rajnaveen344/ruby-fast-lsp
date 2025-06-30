use super::integration_test::{snapshot_references, TestHarness};

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn goto_const_refs() {
        let harness = TestHarness::new().await;
        harness.open_fixture_dir("goto/const_single.rb").await;

        // MyMod definition → module references
        snapshot_references(&harness, "goto/const_single.rb", 0, 7, "my_mod_ref").await;

        // VALUE constant definition → constant references
        snapshot_references(&harness, "goto/const_single.rb", 1, 2, "value_const_ref").await;

        // MyMod::Foo definition → class references
        snapshot_references(&harness, "goto/const_single.rb", 3, 8, "foo_class_ref").await;
    }

    /// Validate references for nested constant paths in goto/nested_const_single.rb fixture.
    #[tokio::test]
    async fn goto_nested_const_refs() {
        let harness = TestHarness::new().await;
        harness
            .open_fixture_dir("goto/nested_const_single.rb")
            .await;

        // ABC constant definition → constant references
        snapshot_references(
            &harness,
            "goto/nested_const_single.rb",
            1,
            4,
            "abc_const_ref",
        )
        .await;

        // Alpha::Beta::Gamma::Foo definition → class references
        snapshot_references(
            &harness,
            "goto/nested_const_single.rb",
            3,
            10,
            "nested_foo_class_ref",
        )
        .await;

        // Alpha namespace in Gamma module definition – expect references assuming defined elsewhere
        snapshot_references(
            &harness,
            "goto/nested_const_single.rb",
            0,
            7,
            "alpha_namespace_ref",
        )
        .await;

        // Beta namespace in Gamma module definition – expect references assuming defined elsewhere
        snapshot_references(
            &harness,
            "goto/nested_const_single.rb",
            0,
            14,
            "beta_namespace_ref",
        )
        .await;
    }
}
