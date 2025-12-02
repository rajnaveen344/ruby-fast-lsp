//! # Comprehensive Generators
//!
//! Tests ALL verification points: definitions, references, types, and completions.

use super::state::GeneratorState;
use super::tracked_v2::TrackedCodeV2;
use proptest::prelude::*;

// =============================================================================
// COMPREHENSIVE MIXIN GENERATOR - Tests ALL verification points
// =============================================================================
//
// This generator creates code that tests:
// 1. Definitions - module, class, method definitions
// 2. References - include statements resolve to module definitions
// 3. Types - method return types, variable assignment types
// 4. Completions - mixin methods appear on class instances

/// Comprehensive mixin test with all verification points
///
/// Creates a deep include chain and tests:
/// - Module/class definitions
/// - Include reference resolution
/// - Method return type inference
/// - Instance variable types
/// - Completion of mixin methods on instance
pub fn graph_comprehensive_mixin() -> impl Strategy<Value = TrackedCodeV2> {
    // depth: how many modules in the include chain
    (2..5usize).prop_map(|depth| {
        let mut state = GeneratorState::new();
        let mut module_names = Vec::new();
        let mut all_methods: Vec<String> = Vec::new();

        // Create the base module with a method that returns a known type
        let base_module = state.make_module_name();
        state.emit(&format!("module {}", base_module));
        let base_method = state.make_method_name();
        state.define_method_with_return(&base_module, &base_method, "\"from base\"", "String");
        state.emit("end");
        state.emit("");
        state.modules.push(base_module.clone());
        state
            .methods
            .insert(base_module.clone(), vec![base_method.clone()]);
        module_names.push(base_module.clone());
        all_methods.push(base_method.clone());

        // Create intermediate modules that include the previous one
        for i in 1..depth {
            let module_name = state.make_module_name();
            state.emit(&format!("module {}", module_name));
            state.add_include(&module_names[i - 1]); // Reference to previous module

            // Add a method to this module too
            let method_name = state.make_method_name();
            let return_value = format!("{}", 100 + i);
            state.define_method_with_return(&module_name, &method_name, &return_value, "Integer");

            state.emit("end");
            state.emit("");

            state.modules.push(module_name.clone());
            state
                .methods
                .insert(module_name.clone(), vec![method_name.clone()]);
            module_names.push(module_name);
            all_methods.push(method_name);
        }

        // Create the final class that includes the last module
        let class_name = state.make_class_name();
        state.emit(&format!("class {}", class_name));

        // Include the last module in the chain
        state.add_include(module_names.last().unwrap());

        // Add a class-specific method
        let class_method = state.make_method_name();
        state.define_method_with_return(&class_name, &class_method, "[1, 2, 3]", "Array");
        all_methods.push(class_method.clone());

        // Add a method that calls the mixin method and assigns result
        state.emit("  def use_mixin_methods");
        let result_var = state.make_var_name();
        state.typed_method_call(&result_var, "self", &base_method, "String");
        state.emit(&format!("    {}", result_var));
        state.emit("  end");
        all_methods.push("use_mixin_methods".to_string());

        state.emit("end");
        state.emit("");
        state.classes.push(class_name.clone());
        state.methods.insert(
            class_name.clone(),
            vec![class_method, "use_mixin_methods".to_string()],
        );

        // Create an instance and test completions
        let instance_var = state.make_var_name();
        state.emit(&format!("{} = {}.new", instance_var, class_name));

        // Add completion trigger on the instance
        state.add_var_completion_trigger(&instance_var, all_methods);

        // Test type inference on method call result
        let result_var2 = state.make_var_name();
        state.typed_method_call(&result_var2, &instance_var, &base_method, "String");

        TrackedCodeV2::from_state(state, "comprehensive_mixin.rb".to_string())
    })
}

/// Comprehensive class with method return type verification
///
/// Creates a class with multiple methods that return different types
/// and verifies type inference on method call results
pub fn graph_method_return_types() -> impl Strategy<Value = TrackedCodeV2> {
    Just(()).prop_map(|_| {
        let mut state = GeneratorState::new();

        let class_name = state.make_class_name();
        state.emit(&format!("class {}", class_name));

        // Method returning String
        let string_method = state.make_method_name();
        state.define_method_with_return(&class_name, &string_method, "\"hello world\"", "String");

        // Method returning Integer
        let int_method = state.make_method_name();
        state.define_method_with_return(&class_name, &int_method, "42", "Integer");

        // Method returning Array
        let array_method = state.make_method_name();
        state.define_method_with_return(&class_name, &array_method, "[1, 2, 3]", "Array");

        // Method returning Hash
        let hash_method = state.make_method_name();
        state.define_method_with_return(&class_name, &hash_method, "{ a: 1, b: 2 }", "Hash");

        state.emit("end");
        state.emit("");

        state.classes.push(class_name.clone());
        state.methods.insert(
            class_name.clone(),
            vec![
                string_method.clone(),
                int_method.clone(),
                array_method.clone(),
                hash_method.clone(),
            ],
        );

        // Create instance
        let obj = state.make_var_name();
        state.emit(&format!("{} = {}.new", obj, class_name));
        state.emit("");

        // Test return type inference for each method
        let str_result = state.make_var_name();
        state.typed_method_call(&str_result, &obj, &string_method, "String");

        let int_result = state.make_var_name();
        state.typed_method_call(&int_result, &obj, &int_method, "Integer");

        let arr_result = state.make_var_name();
        state.typed_method_call(&arr_result, &obj, &array_method, "Array");

        let hash_result = state.make_var_name();
        state.typed_method_call(&hash_result, &obj, &hash_method, "Hash");

        // Test completion on instance
        state.emit("");
        state.add_var_completion_trigger(
            &obj,
            vec![string_method, int_method, array_method, hash_method],
        );

        TrackedCodeV2::from_state(state, "method_return_types.rb".to_string())
    })
}

/// Comprehensive inheritance test
///
/// Creates a class hierarchy and tests:
/// - Subclass method inheritance
/// - Method override verification
/// - Type inference through inheritance
pub fn graph_comprehensive_inheritance() -> impl Strategy<Value = TrackedCodeV2> {
    Just(()).prop_map(|_| {
        let mut state = GeneratorState::new();

        // Base class with methods
        let base_class = state.make_class_name();
        state.emit(&format!("class {}", base_class));

        let base_method = state.make_method_name();
        state.define_method_with_return(&base_class, &base_method, "\"from base\"", "String");

        let shared_method = state.make_method_name();
        state.define_method_with_return(&base_class, &shared_method, "0", "Integer");

        state.emit("end");
        state.emit("");
        state.classes.push(base_class.clone());
        state.methods.insert(
            base_class.clone(),
            vec![base_method.clone(), shared_method.clone()],
        );

        // Subclass that inherits and adds methods - use open_subclass to keep it open
        let sub_class = state.make_class_name();
        state.open_subclass(&sub_class, &base_class);

        // Override the shared method with different return value (same type)
        state.emit(&format!("  def {}", shared_method));
        state.emit("    999");
        state.emit("  end");
        state.emit("");

        // Add subclass-specific method
        let sub_method = state.make_method_name();
        state.define_method_with_return(&sub_class, &sub_method, "[\"a\", \"b\"]", "Array");

        state.emit("end");
        state.emit("");
        // Note: classes and methods already added by open_subclass
        if let Some(methods) = state.methods.get_mut(&sub_class) {
            methods.push(sub_method.clone());
            methods.push(shared_method.clone());
        }

        // Test on subclass instance
        let obj = state.make_var_name();
        state.emit(&format!("{} = {}.new", obj, sub_class));
        state.emit("");

        // Should have base method via inheritance
        let result1 = state.make_var_name();
        state.typed_method_call(&result1, &obj, &base_method, "String");

        // Should have overridden method
        let result2 = state.make_var_name();
        state.typed_method_call(&result2, &obj, &shared_method, "Integer");

        // Should have subclass method
        let result3 = state.make_var_name();
        state.typed_method_call(&result3, &obj, &sub_method, "Array");

        // Completion should include all methods
        state.emit("");
        state.add_var_completion_trigger(&obj, vec![base_method, shared_method, sub_method]);

        TrackedCodeV2::from_state(state, "comprehensive_inheritance.rb".to_string())
    })
}
