//! # Graph Growth Generators
//!
//! Pool-based code generation using the Graph Growth strategy.
//! Also includes strict type inference generators that test edge cases.

use super::state::GeneratorState;
use super::tracked_v2::TrackedCodeV2;
use proptest::prelude::*;

// =============================================================================
// GRAPH GROWTH GENERATORS - Pool-Based Code Generation
// =============================================================================

/// Generate a TrackedCodeV2 with a class hierarchy using Graph Growth
pub fn graph_class_hierarchy() -> impl Strategy<Value = TrackedCodeV2> {
    // Parameters: number of classes (1-4), whether to use inheritance
    (1..5usize, prop::bool::ANY).prop_map(|(num_classes, use_inheritance)| {
        let mut state = GeneratorState::new();

        // Generate the first class (always a base class)
        let first_class = state.make_class_name();
        state.emit(&format!("class {}", first_class));
        let method1 = state.make_method_name();
        state.define_method(&first_class, &method1);
        state.close_class();
        state.classes.push(first_class.clone());
        state.methods.insert(first_class.clone(), vec![method1]);

        // Generate additional classes
        for _ in 1..num_classes {
            let class_name = state.make_class_name();

            if use_inheritance && !state.classes.is_empty() {
                // Pick a random parent from existing classes
                let parent_idx = (state.next_id() as usize) % state.classes.len();
                let parent = state.classes[parent_idx].clone();
                state.define_subclass(&class_name, &parent);
            } else {
                state.define_base_class(&class_name);
            }
        }

        TrackedCodeV2::from_state(state, "hierarchy.rb".to_string())
    })
}

/// Generate a TrackedCodeV2 with mixin relationships
pub fn graph_mixin_relationships() -> impl Strategy<Value = TrackedCodeV2> {
    (1..4usize, 1..4usize).prop_map(|(num_modules, num_classes)| {
        let mut state = GeneratorState::new();

        // Generate modules first
        for _ in 0..num_modules {
            let module_name = state.make_module_name();
            state.emit(&format!("module {}", module_name));

            // Add a method to each module
            let method = state.make_method_name();
            state.define_method(&module_name, &method);

            state.close_class();
            state.modules.push(module_name.clone());
            state.methods.insert(module_name, vec![method]);
        }

        // Generate classes that include modules
        for _ in 0..num_classes {
            let class_name = state.make_class_name();
            state.open_class(&class_name);

            // Include a random module
            if !state.modules.is_empty() {
                let mod_idx = (state.next_id() as usize) % state.modules.len();
                let module_name = state.modules[mod_idx].clone();
                state.add_include(&module_name);
            }

            // Add a method to the class
            let method = state.make_method_name();
            state.define_method(&class_name, &method);

            state.close_class();
            state.classes.push(class_name.clone());
        }

        TrackedCodeV2::from_state(state, "mixins.rb".to_string())
    })
}

/// Generate a TrackedCodeV2 with type-inferred variables
pub fn graph_type_inference() -> impl Strategy<Value = TrackedCodeV2> {
    prop_oneof![Just("String"), Just("Integer"), Just("Array"), Just("Hash"),].prop_map(
        |var_type| {
            let mut state = GeneratorState::new();

            let var_name = state.make_var_name();
            let value = match var_type {
                "String" => "\"hello\"",
                "Integer" => "42",
                "Array" => "[1, 2, 3]",
                "Hash" => "{ a: 1 }",
                _ => "nil",
            };

            state.assign_variable(&var_name, value, var_type, 0);
            state.emit("");

            // Add a usage of the variable
            state.emit(&format!("puts {}", var_name));

            TrackedCodeV2::from_state(state, "types.rb".to_string())
        },
    )
}

/// Generate a TrackedCodeV2 with class references (for go-to-definition testing)
pub fn graph_class_references() -> impl Strategy<Value = TrackedCodeV2> {
    (2..5usize).prop_map(|num_classes| {
        let mut state = GeneratorState::new();

        // Generate classes
        for _ in 0..num_classes {
            let class_name = state.make_class_name();
            state.define_base_class(&class_name);
        }

        // Generate references to each class
        for class_name in state.classes.clone() {
            let (ref_code, _anchor) = state.make_class_reference(&class_name);
            state.emit(&format!("_ = {}", ref_code));
        }

        TrackedCodeV2::from_state(state, "references.rb".to_string())
    })
}

/// Generate a TrackedCodeV2 with completion test points
pub fn graph_completion_test() -> impl Strategy<Value = TrackedCodeV2> {
    (2..5usize).prop_map(|num_methods| {
        let mut state = GeneratorState::new();

        let class_name = state.make_class_name();
        state.emit(&format!("class {}", class_name));

        // Generate methods - note: we need to track them in state.methods
        // BEFORE calling add_completion_trigger so the expected methods are populated
        let mut methods = Vec::new();
        for _ in 0..num_methods {
            let method = state.make_method_name();
            state.define_method(&class_name, &method);
            methods.push(method);
        }

        // Register methods BEFORE adding completion trigger
        state.classes.push(class_name.clone());
        state.methods.insert(class_name.clone(), methods);

        // Add a method with completion trigger
        state.emit("  def test_completion");
        let _comp_anchor = state.add_completion_trigger(&class_name);
        state.emit("  end");

        state.close_class();

        TrackedCodeV2::from_state(state, "completion.rb".to_string())
    })
}

/// Generate any TrackedCodeV2 using Graph Growth strategy
pub fn graph_tracked_code() -> impl Strategy<Value = TrackedCodeV2> {
    prop_oneof![
        3 => graph_class_hierarchy(),
        3 => graph_mixin_relationships(),
        2 => graph_type_inference(),
        2 => graph_class_references(),
        2 => graph_completion_test(),
    ]
}

// =============================================================================
// STRICT TYPE INFERENCE GENERATORS - Tests that SHOULD expose bugs
// =============================================================================
//
// These generators create scenarios known to stress type inference:
// - Method chains on literals
// - Array/Hash element access
// - Instance variable types across methods
// - Type propagation through assignments

/// A strict type expectation that MUST match (not lenient)
#[derive(Debug, Clone)]
pub struct StrictTypeExpectation {
    /// Variable or expression name
    pub var_name: String,
    /// Expected type (e.g., "String", "Integer", "Array<String>")
    pub expected_type: String,
    /// Anchor ID for locating in source
    pub anchor_id: String,
    /// Description of what we're testing
    pub description: String,
}

/// Extended ledger for strict type verification
#[derive(Debug, Clone, Default)]
pub struct StrictTypeLedger {
    pub expectations: Vec<StrictTypeExpectation>,
}

/// Generate method chain scenarios that test type propagation
///
/// Tests cases like:
/// - `"hello".upcase` -> String
/// - `[1,2,3].first` -> Integer (or nil)
/// - `{a: 1}.keys` -> Array
pub fn graph_method_chain_types() -> impl Strategy<Value = TrackedCodeV2> {
    prop_oneof![
        // String method chains
        Just(("\"hello\".upcase", "String", "string_upcase")),
        Just(("\"hello\".downcase", "String", "string_downcase")),
        Just(("\"hello\".length", "Integer", "string_length")),
        Just(("\"hello\".chars", "Array", "string_chars")),
        Just(("\"hello\".split", "Array", "string_split")),
        // Array method chains
        Just(("[1, 2, 3].length", "Integer", "array_length")),
        Just(("[1, 2, 3].first", "Integer", "array_first")), // Known issue: might be nil
        Just(("[1, 2, 3].last", "Integer", "array_last")),
        Just(("[1, 2, 3].reverse", "Array", "array_reverse")),
        Just(("[[1], [2]].flatten", "Array", "array_flatten")),
        // Hash method chains
        Just(("{ a: 1 }.keys", "Array", "hash_keys")),
        Just(("{ a: 1 }.values", "Array", "hash_values")),
        Just(("{ a: 1 }.length", "Integer", "hash_length")),
        // Chained methods (more complex)
        Just(("\"hello\".upcase.downcase", "String", "chained_string")),
        Just(("[1, 2, 3].first.to_s", "String", "array_first_to_s")), // This likely fails
        Just(("{ a: 1 }[:a].to_s", "String", "hash_access_to_s")),    // This likely fails
    ]
    .prop_map(|(expr, expected_type, test_name)| {
        let mut state = GeneratorState::new();
        let var_name = state.make_var_name();

        state.emit(&format!(
            "# Test: {} should be {}",
            test_name, expected_type
        ));
        state.emit_typed_assignment(&var_name, expr, expected_type);
        state.emit("");
        state.emit(&format!("puts {}", var_name));

        TrackedCodeV2::from_state(state, format!("{}.rb", test_name))
    })
}

/// Generate array element access scenarios
///
/// Tests:
/// - `["a", "b"][0]` -> String (but inference might say nil or unknown)
/// - `[1, 2, 3][0]` -> Integer
pub fn graph_array_access_types() -> impl Strategy<Value = TrackedCodeV2> {
    prop_oneof![
        // String arrays
        Just((
            "[\"a\", \"b\", \"c\"]",
            "0",
            "String",
            "string_array_access"
        )),
        Just((
            "[\"hello\", \"world\"]",
            "1",
            "String",
            "string_array_second"
        )),
        // Integer arrays
        Just(("[1, 2, 3]", "0", "Integer", "int_array_access")),
        Just(("[10, 20, 30]", "-1", "Integer", "int_array_negative")),
        // Mixed - these are harder to type
        Just(("[:a, :b]", "0", "Symbol", "symbol_array_access")),
    ]
    .prop_map(|(array_literal, index, expected_type, test_name)| {
        let mut state = GeneratorState::new();
        let var_name = state.make_var_name();

        state.emit(&format!(
            "# Test: {}[{}] should be {}",
            array_literal, index, expected_type
        ));
        let expr = format!("{}[{}]", array_literal, index);
        state.emit_typed_assignment(&var_name, &expr, expected_type);
        state.emit("");

        // Try to call a method that requires the expected type
        match expected_type {
            "String" => {
                let result_var = state.make_var_name();
                state.emit(&format!(
                    "{} = {}.upcase # Should work if {} is String",
                    result_var, var_name, var_name
                ));
            }
            "Integer" => {
                let result_var = state.make_var_name();
                state.emit(&format!(
                    "{} = {} + 1 # Should work if {} is Integer",
                    result_var, var_name, var_name
                ));
            }
            _ => {}
        }

        TrackedCodeV2::from_state(state, format!("{}.rb", test_name))
    })
}

/// Generate class instance method type scenarios
///
/// Tests that method calls on class instances return correct types
pub fn graph_class_method_types() -> impl Strategy<Value = TrackedCodeV2> {
    Just(()).prop_map(|_| {
        let mut state = GeneratorState::new();

        // Define a class with typed methods
        let class_name = state.make_class_name();
        state.emit(&format!("class {}", class_name));
        state.emit("  def get_string");
        state.emit("    \"hello\"");
        state.emit("  end");
        state.emit("");
        state.emit("  def get_number");
        state.emit("    42");
        state.emit("  end");
        state.emit("");
        state.emit("  def get_array");
        state.emit("    [1, 2, 3]");
        state.emit("  end");
        state.emit("end");
        state.emit("");

        // Create instance and call methods
        let instance_var = state.make_var_name();
        state.emit(&format!("{} = {}.new", instance_var, class_name));
        state.emit("");

        // These should have known return types based on method body analysis
        let str_var = state.make_var_name();
        state.emit_typed_assignment(&str_var, &format!("{}.get_string", instance_var), "String");

        let num_var = state.make_var_name();
        state.emit_typed_assignment(&num_var, &format!("{}.get_number", instance_var), "Integer");

        let arr_var = state.make_var_name();
        state.emit_typed_assignment(&arr_var, &format!("{}.get_array", instance_var), "Array");

        state.emit("");
        state.emit("# Chain method calls - these test type propagation");
        let chain_var = state.make_var_name();
        state.emit_typed_assignment(
            &chain_var,
            &format!("{}.get_string.upcase", instance_var),
            "String",
        );

        state.classes.push(class_name);
        TrackedCodeV2::from_state(state, "class_method_types.rb".to_string())
    })
}

/// Generate scenarios with instance variables across methods
///
/// Tests that @ivar types are tracked correctly between methods
pub fn graph_ivar_type_propagation() -> impl Strategy<Value = TrackedCodeV2> {
    Just(()).prop_map(|_| {
        let mut state = GeneratorState::new();

        let class_name = state.make_class_name();
        state.emit(&format!("class {}", class_name));

        // Initialize with a known type
        state.emit("  def initialize");
        state.emit("    @data = \"initial string\""); // String
        state.emit("    @numbers = [1, 2, 3]"); // Array<Integer>
        state.emit("  end");
        state.emit("");

        // Method that uses the ivars - type should propagate
        state.emit("  def process");
        let result_var = state.make_var_name();
        // @data.upcase should be String if @data is String
        state.emit(&format!(
            "    {} = @data.upcase # <TYPE:ivar_string>",
            result_var
        ));
        state
            .type_ledger
            .var_types
            .insert(result_var.clone(), "String".to_string());

        let sum_var = state.make_var_name();
        // @numbers.first should be Integer if @numbers is Array<Integer>
        state.emit(&format!(
            "    {} = @numbers.first # <TYPE:ivar_array>",
            sum_var
        ));
        state
            .type_ledger
            .var_types
            .insert(sum_var.clone(), "Integer".to_string());

        state.emit(&format!("    {}", result_var));
        state.emit("  end");
        state.emit("end");
        state.emit("");

        // Test usage
        let obj_var = state.make_var_name();
        state.emit(&format!("{} = {}.new", obj_var, class_name));
        let call_result = state.make_var_name();
        state.emit_typed_assignment(&call_result, &format!("{}.process", obj_var), "String");

        state.classes.push(class_name);
        TrackedCodeV2::from_state(state, "ivar_propagation.rb".to_string())
    })
}

/// Generate completion test for method chains
///
/// After `str = "hello"`, typing `str.` should suggest String methods
pub fn graph_completion_after_type() -> impl Strategy<Value = TrackedCodeV2> {
    prop_oneof![
        Just((
            "\"hello\"",
            "String",
            vec!["upcase", "downcase", "length", "chars"]
        )),
        Just((
            "[1, 2, 3]",
            "Array",
            vec!["first", "last", "length", "each"]
        )),
        Just(("{ a: 1 }", "Hash", vec!["keys", "values", "each"])),
        Just(("42", "Integer", vec!["to_s", "times", "abs"])),
    ]
    .prop_map(|(literal, type_name, expected_methods)| {
        let mut state = GeneratorState::new();
        let var_name = state.make_var_name();

        state.emit(&format!("# {} should have {} methods", var_name, type_name));
        state.assign_variable(&var_name, literal, type_name, 0);
        state.emit("");

        // Completion trigger
        let comp_anchor = state.make_completion_anchor();
        state.emit(&format!("{}. # <{}>", var_name, comp_anchor));

        // Record expected completions
        state.completion_ledger.expected_completions.insert(
            comp_anchor,
            expected_methods.iter().map(|s| s.to_string()).collect(),
        );

        TrackedCodeV2::from_state(state, format!("{}_completion.rb", type_name.to_lowercase()))
    })
}

/// Generate edge cases known to break type inference
///
/// These are scenarios where type inference is known to fail
pub fn graph_type_edge_cases() -> impl Strategy<Value = TrackedCodeV2> {
    prop_oneof![
        // Conditional assignment - type might be union
        Just((
            "x = rand > 0.5 ? \"string\" : 42",
            "union",
            "conditional_type"
        )),
        // Nil coalescing
        Just(("x = nil || \"default\"", "String", "nil_coalesce")),
        // Array of mixed types
        Just((
            "arr = [1, \"two\", :three]; x = arr[0]",
            "unknown",
            "mixed_array"
        )),
        // Method with no obvious return type
        Just((
            "def mystery; if rand > 0.5; 1; else; \"a\"; end; end; x = mystery",
            "unknown",
            "mystery_method"
        )),
    ]
    .prop_map(|(code, expected_category, test_name)| {
        let mut state = GeneratorState::new();

        state.emit(&format!(
            "# Edge case: {} (expected: {})",
            test_name, expected_category
        ));
        // For edge cases, we just emit the code and don't expect precise types
        for line in code.split("; ") {
            state.emit(line);
        }

        TrackedCodeV2::from_state(state, format!("{}.rb", test_name))
    })
}

