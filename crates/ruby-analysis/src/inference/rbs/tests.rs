use super::catalog::rbs_method_name_catalog;
use super::embedded_callable::prepare_rbs_higher_order_call;
use super::*;
use crate::core::RubyType;
use rbs_parser::RbsType;

#[test]
fn test_rbs_loader_initialized() {
    let count = RBS_LOADER.read().declaration_count();
    assert!(count > 0, "RBS loader should have declarations");
    println!("Loaded {} RBS declarations", count);
}

#[test]
fn test_string_length_return_type() {
    let return_type = get_rbs_method_return_type("String", "length", false);
    assert!(
        return_type.is_some(),
        "String#length should have a return type"
    );
    if let Some(RbsType::Class(name)) = return_type {
        assert_eq!(name, "Integer");
    } else {
        panic!("Expected Integer return type, got {:?}", return_type);
    }
}

#[test]
fn test_string_upcase_return_type() {
    let return_type = get_rbs_method_return_type("String", "upcase", false);
    assert!(
        return_type.is_some(),
        "String#upcase should have a return type"
    );
    // String#upcase returns `self?` in RBS (optional self type)
    // This is correct - it returns the same string
    println!("String#upcase return type: {:?}", return_type);
}

#[test]
fn test_string_downcase_return_type() {
    let return_type = get_rbs_method_return_type("String", "downcase", false);
    assert!(
        return_type.is_some(),
        "String#downcase should have a return type"
    );
    println!("String#downcase return type: {:?}", return_type);
}

#[test]
fn intersection_type_does_not_select_an_arbitrary_member() {
    let rbs_type = RbsType::Intersection(vec![
        RbsType::Class("String".to_string()),
        RbsType::Class("Integer".to_string()),
    ]);

    assert_eq!(rbs_type_to_ruby_type(&rbs_type), RubyType::Unknown);
}

#[test]
fn union_with_untyped_member_remains_unknown() {
    let rbs_type = RbsType::Union(vec![RbsType::Class("String".to_string()), RbsType::Untyped]);

    assert_eq!(rbs_type_to_ruby_type(&rbs_type), RubyType::Unknown);
}

#[test]
fn test_integer_to_s_return_type() {
    let return_type = get_rbs_method_return_type("Integer", "to_s", false);
    assert!(
        return_type.is_some(),
        "Integer#to_s should have a return type"
    );
    if let Some(RbsType::Class(name)) = return_type {
        assert_eq!(name, "String");
    } else {
        panic!("Expected String return type, got {:?}", return_type);
    }
}

#[test]
fn test_array_first_return_type() {
    let return_type = get_rbs_method_return_type("Array", "first", false);
    assert!(
        return_type.is_some(),
        "Array#first should have a return type"
    );
    println!("Array#first return type: {:?}", return_type);
}

#[test]
fn test_has_string_class() {
    assert!(has_rbs_class("String"), "Should have String class");
    assert!(has_rbs_class("Integer"), "Should have Integer class");
    assert!(has_rbs_class("Array"), "Should have Array class");
    assert!(has_rbs_class("Hash"), "Should have Hash class");
}

#[test]
fn test_class_has_new_method() {
    assert!(has_rbs_class("Class"), "Should have Class class in RBS");
    let methods = get_rbs_class_methods("Class", false);
    let method_names: Vec<&str> = methods.iter().map(|m| m.name.as_str()).collect();
    assert!(
        method_names.contains(&"new"),
        "Class should have 'new' instance method. Found: {:?}",
        method_names
    );
}

#[test]
fn test_nonexistent_method() {
    let return_type = get_rbs_method_return_type("String", "nonexistent_method_xyz", false);
    assert!(return_type.is_none(), "Should not find nonexistent method");
}

#[test]
fn test_string_chars_return_type() {
    let return_type = get_rbs_method_return_type("String", "chars", false);
    println!("String#chars return type: {:?}", return_type);
    assert!(
        return_type.is_some(),
        "String#chars should have a return type"
    );

    // Also test the RubyType conversion
    let ruby_type = get_rbs_method_return_type_as_ruby_type("String", "chars", false);
    println!("String#chars as RubyType: {:?}", ruby_type);
}

#[test]
fn test_nil_class_inherits_object_methods() {
    let methods = get_rbs_class_methods("NilClass", false);
    let method_names: Vec<&str> = methods.iter().map(|m| m.name.as_str()).collect();

    // NilClass's own methods
    assert!(method_names.contains(&"nil?"), "Should have NilClass#nil?");
    assert!(method_names.contains(&"to_s"), "Should have NilClass#to_s");
    assert!(
        method_names.contains(&"inspect"),
        "Should have NilClass#inspect"
    );

    // Inherited from Object/Kernel/BasicObject
    assert!(
        method_names.contains(&"class"),
        "Should have Object#class (inherited)"
    );
    assert!(
        method_names.contains(&"is_a?"),
        "Should have Kernel#is_a? (inherited)"
    );
    assert!(
        method_names.contains(&"freeze"),
        "Should have Object#freeze (inherited)"
    );
    assert!(
        method_names.contains(&"respond_to?"),
        "Should have Kernel#respond_to? (inherited)"
    );
    assert!(
        method_names.contains(&"tap"),
        "Should have Kernel#tap (inherited)"
    );
    assert!(
        method_names.contains(&"public_send"),
        "Should have Kernel#public_send (inherited)"
    );
    // Aliases should also be resolved
    assert!(
        method_names.contains(&"object_id"),
        "Should have Kernel#object_id (alias of __id__)"
    );
}

#[test]
fn test_string_inherits_object_methods() {
    let methods = get_rbs_class_methods("String", false);
    let method_names: Vec<&str> = methods.iter().map(|m| m.name.as_str()).collect();

    // String's own methods
    assert!(
        method_names.contains(&"upcase"),
        "Should have String#upcase"
    );
    assert!(
        method_names.contains(&"length"),
        "Should have String#length"
    );

    // Inherited from Object/Kernel
    assert!(
        method_names.contains(&"class"),
        "Should have Object#class (inherited)"
    );
    assert!(
        method_names.contains(&"is_a?"),
        "Should have Kernel#is_a? (inherited)"
    );
    assert!(
        method_names.contains(&"tap"),
        "Should have Kernel#tap (inherited)"
    );
}

#[test]
fn rbs_method_catalog_is_shared_and_preserves_inherited_aliases() {
    let first = rbs_method_name_catalog("String", false);
    let second = rbs_method_name_catalog("String", false);

    invariant!(
        std::sync::Arc::ptr_eq(&first, &second),
        what = "repeated immutable RBS catalog queries rebuilt String",
        why = "the embedded RBS environment cannot change at runtime",
        fix = "share one catalog per declared RBS owner and singleton mode",
    );
    invariant!(
        first.contains("tap"),
        what = "cached String methods lost inherited Kernel#tap",
        why = "caching must preserve the complete RBS ancestor lookup",
        fix = "construct the cache entry through the ordinary recursive collector",
    );
    invariant!(
        first.contains("object_id"),
        what = "cached String methods lost the Kernel#object_id alias",
        why = "cached and uncached RBS lookup must be semantically identical",
        fix = "retain alias collection when constructing a cache entry",
    );
}

#[test]
fn test_generic_type_substitution_array_first() {
    let type_args = vec![RubyType::integer()];
    let result = get_rbs_method_return_type_with_type_args("Array", "first", false, &type_args);
    assert!(
        result.is_some(),
        "Array#first with type_args should return a type"
    );
    let rt = result.unwrap();
    assert_eq!(
        rt,
        RubyType::integer(),
        "Array[Integer]#first should return Integer"
    );
}

#[test]
fn test_generic_type_substitution_hash_keys() {
    let type_args = vec![RubyType::symbol(), RubyType::string()];
    let result = get_rbs_method_return_type_with_type_args("Hash", "keys", false, &type_args);
    assert!(result.is_some(), "Hash#keys should return a type");
    let rt = result.unwrap();
    // Hash[Symbol, String]#keys should return Array[Symbol]
    assert_eq!(
        rt,
        RubyType::Array(vec![RubyType::symbol()]),
        "Hash[Symbol, String]#keys should return Array[Symbol]"
    );
}

#[test]
fn array_map_callable_uses_the_generic_enumerable_include() {
    let prepared =
        prepare_rbs_higher_order_call(&RubyType::array_of(RubyType::integer()), "map", &[])
            .expect("Array#map must resolve through Enumerable[Elem]");
    assert_eq!(prepared.block_parameter_types(), &[RubyType::integer()]);
    assert_eq!(
        prepared.finish(&RubyType::string()).proven_type(),
        Some(&RubyType::array_of(RubyType::string()))
    );
}

#[test]
fn array_each_callable_binds_the_receiver_element() {
    let prepared =
        prepare_rbs_higher_order_call(&RubyType::array_of(RubyType::integer()), "each", &[])
            .expect("Array#each must bind its receiver element generic");
    assert_eq!(prepared.block_parameter_types(), &[RubyType::integer()]);
}

#[test]
fn enumerable_filter_map_callable_subtracts_falsey_results() {
    let prepared =
        prepare_rbs_higher_order_call(&RubyType::array_of(RubyType::integer()), "filter_map", &[])
            .expect("Array#filter_map must resolve through Enumerable[Elem]");
    let block_result = RubyType::union([RubyType::nil_class(), RubyType::string()]);
    assert_eq!(
        prepared.finish(&block_result).proven_type(),
        Some(&RubyType::array_of(RubyType::string()))
    );
}

#[test]
fn enumerable_each_with_object_binds_the_accumulator_argument() {
    let accumulator = RubyType::hash_of(RubyType::symbol(), RubyType::integer());
    let prepared = prepare_rbs_higher_order_call(
        &RubyType::array_of(RubyType::integer()),
        "each_with_object",
        std::slice::from_ref(&accumulator),
    )
    .expect("Array#each_with_object must resolve through Enumerable[Elem]");
    assert_eq!(
        prepared.block_parameter_types(),
        &[RubyType::integer(), accumulator.clone()]
    );
    assert_eq!(
        prepared.finish(&RubyType::integer()).proven_type(),
        Some(&accumulator)
    );
}

#[test]
fn higher_order_prepare_cache_reuses_identical_array_each_calls() {
    let receiver = RubyType::array_of(RubyType::integer());
    let first =
        prepare_higher_order_call_with_fallbacks(None, None, Some(&receiver), None, "each", &[])
            .expect("Array#each must prepare through the cached fallback path");
    let second =
        prepare_higher_order_call_with_fallbacks(None, None, Some(&receiver), None, "each", &[])
            .expect("repeated Array#each must hit the same prepare cache");
    assert_eq!(
        first.block_parameter_types(),
        second.block_parameter_types()
    );
    assert_eq!(first.block_parameter_types(), &[RubyType::integer()]);
}

#[test]
fn higher_order_prepare_cache_does_not_reuse_a_different_element_type() {
    let integers = prepare_higher_order_call_with_fallbacks(
        None,
        None,
        Some(&RubyType::array_of(RubyType::integer())),
        None,
        "each",
        &[],
    )
    .expect("Array[Integer]#each must prepare");
    let strings = prepare_higher_order_call_with_fallbacks(
        None,
        None,
        Some(&RubyType::array_of(RubyType::string())),
        None,
        "each",
        &[],
    )
    .expect("Array[String]#each must prepare");
    assert_eq!(integers.block_parameter_types(), &[RubyType::integer()]);
    assert_eq!(strings.block_parameter_types(), &[RubyType::string()]);
}
