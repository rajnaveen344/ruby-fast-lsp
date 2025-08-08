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

    /// Validate method definitions across modules when both modules are included in a class.
    /// Tests the scenario where a method in module A calls a method in module B.
    #[tokio::test]
    async fn goto_module_method_cross_ref() {
        let harness = TestHarness::new().await;
        harness.open_fixture_dir("goto/module_method_cross_ref.rb").await;

        // `method_from_b` call inside ModuleA's method_from_a → method definition in ModuleB
        snapshot_definitions(
            &harness,
            "goto/module_method_cross_ref.rb",
            3,
            4,
            "method_from_b_def_in_module_b",
        )
        .await;

        // `method_from_a` call in TestClass → method definition in ModuleA
        snapshot_definitions(
            &harness,
            "goto/module_method_cross_ref.rb",
            26,
            4,
            "method_from_a_def_in_module_a",
        )
        .await;

        // `method_from_b` call in TestClass → method definition in ModuleB
        snapshot_definitions(
            &harness,
            "goto/module_method_cross_ref.rb",
            27,
            4,
            "method_from_b_def_in_module_b_from_class",
        )
        .await;

        // `helper_method` call in TestClass → method definition in ModuleB
        snapshot_definitions(
            &harness,
            "goto/module_method_cross_ref.rb",
            31,
            4,
            "helper_method_def_in_module_b",
        )
        .await;

        // `method_from_a` call on instance → method definition in ModuleA
        snapshot_definitions(
            &harness,
            "goto/module_method_cross_ref.rb",
            38,
            14,
            "method_from_a_def_instance_call",
        )
        .await;

        // `method_from_b` call on instance → method definition in ModuleB
        snapshot_definitions(
            &harness,
            "goto/module_method_cross_ref.rb",
            39,
            14,
            "method_from_b_def_instance_call",
        )
        .await;
    }

    /// Validate method definitions across modules with partially qualified includes in nested namespaces.
    /// Tests the scenario where modules are included using partially qualified names within a namespace.
    #[tokio::test]
    async fn goto_nested_namespace_include() {
        let harness = TestHarness::new().await;
        harness.open_fixture_dir("goto/nested_namespace_include.rb").await;



        // `method_from_b` call inside Outer::ModuleA's method_from_a → method definition in Outer::ModuleB
        snapshot_definitions(
            &harness,
            "goto/nested_namespace_include.rb",
            4,
            6,
            "method_from_b_def_in_outer_module_b",
        )
        .await;

        // `method_from_a` call in Outer::TestClass → method definition in Outer::ModuleA
        snapshot_definitions(
            &harness,
            "goto/nested_namespace_include.rb",
            19,
            6,
            "method_from_a_def_in_outer_module_a",
        )
        .await;

        // `method_from_b` call in Outer::TestClass → method definition in Outer::ModuleB
        snapshot_definitions(
            &harness,
            "goto/nested_namespace_include.rb",
            20,
            6,
            "method_from_b_def_in_outer_module_b_from_class",
        )
        .await;
    }
}
