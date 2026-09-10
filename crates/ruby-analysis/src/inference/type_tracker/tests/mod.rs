use crate::core::{
    FullyQualifiedName, LiteralKey, LiteralValue, RubyConstant, RubyMethod, RubyType,
    ShapeExactness, ShapeField, ShapeRest, ShapeStability, ShapeType, TypeInferenceOutcome,
    UnknownReason,
};
use crate::inference::type_tracker::flow::shapes::narrowing::narrow_shape_literal_type;
use crate::inference::type_tracker::observations::{get_var_type_at, LocalReadType};
use crate::inference::type_tracker::TypeTracker;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

mod array_aliases;
mod assignments;
mod control_flow;
mod returns;
mod shape_aliases;
mod shape_narrowing;
mod shape_values;

fn instance_type(name: &str) -> RubyType {
    RubyType::Class(FullyQualifiedName::constant(vec![
        RubyConstant::new(name).expect("test class name must be a valid Ruby constant")
    ]))
}

fn tracked_method_type(source: &str) -> RubyType {
    let parse_result = ruby_prism::parse(source.as_bytes());
    let definition = parse_result
        .node()
        .as_program_node()
        .expect("test source must parse as a program")
        .statements()
        .body()
        .iter()
        .next()
        .expect("test source must contain a method")
        .as_def_node()
        .expect("test source must begin with a method definition");
    TypeTracker::new().track_method(&definition)
}

fn exact_local_read_type(tracker: &mut TypeTracker, source: &str, needle: &str) -> RubyType {
    exact_local_read(tracker, source, needle).ruby_type
}

fn exact_local_read(tracker: &mut TypeTracker, source: &str, needle: &str) -> LocalReadType {
    let start_offset = source.rfind(needle).expect(
            "INVARIANT VIOLATED: the test local-read needle is absent. This is a bug because the fixture and assertion must identify the same source token. Fix: keep the needle synchronized with the fixture.",
        );
    tracker
            .take_local_read_types()
            .into_iter()
            .find(|read| read.start_offset == start_offset)
            .expect(
                "INVARIANT VIOLATED: TypeTracker did not retain the expected exact local read. This is a bug because the fixture places it inside rescue control flow. Fix: keep local-read evidence enabled and traverse the rescue expression through track_node.",
            )
}
