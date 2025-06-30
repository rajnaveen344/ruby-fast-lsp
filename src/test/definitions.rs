use super::integration_test::{snapshot_definitions, TestHarness};

#[cfg(test)]
mod tests {
    use super::*;

    /// Validate definitions for module, class and constant in def_ref/single_file fixture.
    #[tokio::test]
    async fn goto_single_file_defs() {
        let harness = TestHarness::new().await;
        harness.open_fixture_dir("goto/const_single.rb").await;

        // MyMod::Foo reference → class definition
        snapshot_definitions(&harness, "goto/const_single.rb", 12, 14, "foo_class_def").await;

        // include MyMod → module definition
        snapshot_definitions(&harness, "goto/const_single.rb", 10, 8, "module_def").await;

        // VALUE constant usage inside method → constant definition
        snapshot_definitions(&harness, "goto/const_single.rb", 5, 6, "value_const_def").await;

        // puts MyMod::VALUE constant usage at top level
        snapshot_definitions(
            &harness,
            "goto/const_single.rb",
            13,
            12,
            "value_const_def_top",
        )
        .await;
    }

    /// Validate definitions for nested constant paths in goto/const_single.rb fixture.
    #[tokio::test]
    async fn goto_nested_const_defs() {
        let harness = TestHarness::new().await;
        harness
            .open_fixture_dir("goto/nested_const_single.rb")
            .await;

        // Alpha::Beta::Gamma::Foo reference → class definition
        snapshot_definitions(
            &harness,
            "goto/nested_const_single.rb",
            11,
            20,
            "nested_foo_class_def",
        )
        .await;

        // ABC constant usage inside method → constant definition
        snapshot_definitions(
            &harness,
            "goto/nested_const_single.rb",
            5,
            8,
            "abc_const_def",
        )
        .await;

        // Alpha constant usage at top level - No definition found
        snapshot_definitions(&harness, "goto/nested_const_single.rb", 10, 0, "alpha_top").await;

        // Alpha::Beta constant usage at top level - No definition found
        snapshot_definitions(&harness, "goto/nested_const_single.rb", 10, 7, "beta_top").await;

        // Alpha::Beta::Gamma constant usage at top level
        snapshot_definitions(&harness, "goto/nested_const_single.rb", 10, 13, "gamma_top").await;

        // Alpha::Beta::Gamma::ABC constant usage at top level
        snapshot_definitions(
            &harness,
            "goto/nested_const_single.rb",
            10,
            20,
            "abc_const_def_top",
        )
        .await;
    }

    /*----------------------------------------------------------------------
     Method fixtures – Greeter#greet (no receiver) and Utils.process (constant
     receiver)
    ----------------------------------------------------------------------*/

    /// Validate definitions for methods without an explicit receiver (i.e.
    /// plain method calls inside the same class).
    #[tokio::test]
    async fn goto_method_defs() {
        let harness = TestHarness::new().await;
        harness.open_fixture_dir("goto/method_single.rb").await;

        // `greet` call inside `run` → method definition
        snapshot_definitions(&harness, "goto/method_single.rb", 18, 8, "greet_method_def").await;

        // `hello` call on `Greeter` class → self method definition
        snapshot_definitions(
            &harness,
            "goto/method_single.rb",
            22,
            8,
            "hello_class_method_def",
        )
        .await;

        // `new` call on `Greeter` class → constructor method definition
        snapshot_definitions(
            &harness,
            "goto/method_single.rb",
            24,
            8,
            "constructor_method_def",
        )
        .await;

        // `hello` call on `Greeter` instance → instance method definition
        snapshot_definitions(
            &harness,
            "goto/method_single.rb",
            24,
            12,
            "hello_instance_method_def",
        )
        .await;

        // `top_method` call at top level → method definition
        snapshot_definitions(&harness, "goto/method_single.rb", 32, 0, "top_method_def").await;
    }
}
