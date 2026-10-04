use super::navigation::supplemental_implementation_location;
use super::*;
use parking_lot::RwLock;
use ruby_analysis::core::{
    FullyQualifiedName, NamespaceKind, ReferenceCandidateKind, RubyConstant, RubyType,
    SourceFileId, SourceKind, SymbolKind, TypeProvenance, TypeSubject,
};
use ruby_analysis::engine::{Project, SourceFileInput};
use ruby_analysis::indexer::fact_collector::FactCollector;
use ruby_analysis::indexer::RubyDocument;
use ruby_fast_lsp_jvm_metadata::{
    ClassFile, JavaSourceClassLocation, JavaSourceMemberLocation, MemberInfo, MethodParameter,
    SourceByteRange,
};
use ruby_prism::Visit;

use tower_lsp::lsp_types::Url;

fn class_files(class_names: &[&str]) -> Vec<ClassFile> {
    class_names
        .iter()
        .map(|name| ClassFile {
            minor_version: 0,
            major_version: 61,
            access_flags: 0x0021,
            name: (*name).into(),
            super_name: Some("java/lang/Object".into()),
            interfaces: Box::default(),
            fields: Box::default(),
            methods: Box::default(),
            source_file: None,
            is_record: false,
        })
        .collect()
}

fn catalog_of(classes: Vec<ClassFile>) -> Arc<ProjectJavaCatalog> {
    Arc::new(ProjectJavaCatalog::from_test_classes(
        "fixture-classpath",
        crate::test::harness::fixture_path("/fixture/runtime.jar"),
        classes,
    ))
}

fn catalog(class_names: &[&str]) -> Arc<ProjectJavaCatalog> {
    catalog_of(class_files(class_names))
}

fn class_mut<'a>(classes: &'a mut [ClassFile], name: &str) -> &'a mut ClassFile {
    classes
        .iter_mut()
        .find(|class| &*class.name == name)
        .expect("fixture class must exist")
}

fn add_methods(class: &mut ClassFile, methods: impl IntoIterator<Item = MemberInfo>) {
    let mut all = class.methods.to_vec();
    all.extend(methods);
    class.methods = all.into_boxed_slice();
}

fn collect(source: &str, class_names: &[&str]) -> FactCollector {
    collect_with_catalog(source, catalog(class_names))
}

#[test]
fn provider_ignores_anonymous_jvm_classes_without_losing_named_nested_proxies() {
    let provider = JrubyImportProvider::new(catalog(&[
        "com/apple/eawt/FullScreenHandler$1",
        "java/util/Map$Entry",
    ]));

    assert_eq!(
        provider
            .class_name_for_static_proxy_reference("Java::JavaUtil::Map::Entry")
            .unwrap(),
        Some("java/util/Map$Entry".to_string())
    );
    assert_eq!(
        provider
            .class_name_for_static_proxy_reference("Java::ComAppleEawt::FullScreenHandler::1")
            .unwrap(),
        None
    );
}

#[test]
fn provider_reports_ambiguous_proxies_and_lists_only_direct_package_classes() {
    let provider = JrubyImportProvider::new(catalog(&[
        "com/example/Widget",
        "comExample/Widget",
        "java/util/List",
        "java/util/Map$Entry",
        "java/util/concurrent/Future",
        "java/utility/Helper",
    ]));

    assert_eq!(
        provider.class_name_for_static_proxy_reference("Java::ComExample::Widget"),
        Err(
            "JRuby proxy `Java::ComExample::Widget` maps to multiple classpath identities: \
             com/example/Widget, comExample/Widget"
                .to_string()
        )
    );
    assert_eq!(
        provider.class_name_for_static_proxy_reference("Java::JavaUtil::List"),
        Ok(Some("java/util/List".to_string()))
    );
    assert_eq!(
        provider.class_names_in_package("java.util"),
        Ok(vec!["java/util/List".to_string()])
    );
    assert!(provider.source_may_reference_static_java("widget = com.example.Widget.new\n"));
    assert!(provider.source_may_reference_static_java("widget = comExample.Widget.new\n"));
    assert!(!provider.source_may_reference_static_java("name = user.profile.name\n"));
}

fn collect_with_catalog(source: &str, catalog: Arc<ProjectJavaCatalog>) -> FactCollector {
    collect_with_provider(source, Arc::new(JrubyImportProvider::new(catalog)))
}

fn collect_with_provider(source: &str, provider: Arc<JrubyImportProvider>) -> FactCollector {
    let path = crate::test::harness::fixture_path("/workspace/admin/imports.rb");
    let uri = Url::from_file_path(&path).expect("fixture path must be a file URI");
    let mut engine = Project::new();
    let file_id = engine.register_file(SourceFileInput {
        path,
        content: source.to_string(),
        kind: SourceKind::Project,
    });
    let engine = Arc::new(RwLock::new(engine));
    let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
    let mut collector = FactCollector::analysis_only(document, provider, engine);
    let parse = ruby_prism::parse(source.as_bytes());
    collector.visit(&parse.node());
    collector
}

#[test]
fn imports_dotted_java_class_into_current_lexical_namespace() {
    let collector = collect(
        "module Admin\n  java_import java.lang.String\nend\n",
        &["java/lang/String"],
    );
    let alias =
        FullyQualifiedName::try_from("Admin::String").expect("fixture alias FQN must be valid");
    assert!(collector
        .analysis()
        .symbols
        .iter()
        .any(|fact| fact.fqn == alias && fact.kind == SymbolKind::Constant));
    assert!(collector.analysis().types.iter().any(|fact| {
        fact.subject == TypeSubject::Constant(alias.clone())
            && fact.ruby_type
                == RubyType::ClassReference(
                    FullyQualifiedName::try_from("Java::JavaLang::String")
                        .expect("fixture proxy FQN must be valid"),
                )
            && fact.provenance == TypeProvenance::Runtime
    }));
    assert!(collector.reference_candidates().iter().any(|candidate| {
        matches!(
            &candidate.kind,
            ReferenceCandidateKind::Resolved { target, .. }
                if *target == FullyQualifiedName::try_from("Java::JavaLang::String").unwrap()
        )
    }));
    assert!(collector.diagnostics().is_empty());
}

#[test]
fn infers_instances_constructed_from_a_canonical_dotted_java_proxy() {
    let collector = collect("INSTANCE = java.lang.String.new\n", &["java/lang/String"]);
    let instance =
        FullyQualifiedName::try_from("INSTANCE").expect("fixture constant must be valid");
    assert!(collector.analysis().types.iter().any(|fact| {
        fact.subject == TypeSubject::Constant(instance.clone())
            && fact.ruby_type
                == RubyType::Class(FullyQualifiedName::try_from("Java::JavaLang::String").unwrap())
    }));
}

#[test]
fn generated_signature_source_preserves_its_java_proxy_class_declaration() {
    let collector = collect(
        "module Java\n\
             \x20 module JavaLang\n\
             \x20   class String < Java::JavaLang::Object\n\
             \x20     def self.new; end\n\
             \x20   end\n\
             \x20 end\n\
             end\n",
        &["java/lang/String"],
    );
    let proxy = FullyQualifiedName::namespace(
        ["Java", "JavaLang", "String"]
            .into_iter()
            .map(|part| RubyConstant::new(part).unwrap())
            .collect::<Vec<_>>(),
    );
    assert!(collector
        .analysis()
        .symbols
        .iter()
        .any(|fact| fact.fqn == proxy && fact.kind == SymbolKind::Class));
}

#[test]
fn java_alias_projects_the_selected_java_overload_onto_the_proxy_owner() {
    let mut classes = class_files(&["java/util/ArrayList", "java/lang/Object"]);
    let declaration = class_mut(&mut classes, "java/util/ArrayList");
    add_methods(
        declaration,
        [MemberInfo {
            access_flags: 0x0001,
            name: "add".into(),
            descriptor: "(ILjava/lang/Object;)Z".into(),
            exceptions: Box::default(),
            parameters: Box::new([
                MethodParameter {
                    name: "index".into(),
                },
                MethodParameter {
                    name: "value".into(),
                },
            ]),
            first_line: None,
        }],
    );
    let collector = collect_with_catalog(
        "java_import java.util.ArrayList\n\
             class ArrayList\n\
               java_alias :simple_add, :add, [Java::int, java.lang.Object]\n\
             end\n",
        catalog_of(classes),
    );
    let alias = FullyQualifiedName::method(
        ["Java", "JavaUtil", "ArrayList"]
            .into_iter()
            .map(|part| RubyConstant::new(part).unwrap())
            .collect::<Vec<_>>(),
        ruby_analysis::core::RubyMethod::new("simple_add").unwrap(),
    );
    let method = collector
        .analysis()
        .methods
        .iter()
        .find(|fact| fact.fqn == alias)
        .expect("java_alias must define the alias on the Java proxy");
    assert!(method.param_names().eq(["index", "value"]));
    assert!(collector.analysis().types.iter().any(|fact| {
        fact.subject == TypeSubject::MethodReturn(alias.clone())
            && fact.ruby_type == RubyType::boolean()
            && fact.provenance == TypeProvenance::Runtime
    }));
    assert!(collector.diagnostics().is_empty());
}

#[test]
fn java_send_selects_an_exact_overload_projects_its_return_and_references_its_name() {
    let mut classes = class_files(&[
        "java/util/ArrayList",
        "java/lang/Object",
        "java/lang/String",
    ]);
    let list = class_mut(&mut classes, "java/util/ArrayList");
    add_methods(
        list,
        [
            MemberInfo {
                access_flags: 0x0001,
                name: "get".into(),
                descriptor: "(I)Ljava/lang/Object;".into(),
                exceptions: Box::default(),
                parameters: Box::new([MethodParameter {
                    name: "index".into(),
                }]),
                first_line: None,
            },
            MemberInfo {
                access_flags: 0x0001,
                name: "get".into(),
                descriptor: "(Ljava/lang/String;)Ljava/lang/String;".into(),
                exceptions: Box::default(),
                parameters: Box::new([MethodParameter { name: "key".into() }]),
                first_line: None,
            },
        ],
    );
    let provider = Arc::new(JrubyImportProvider::new(catalog_of(classes)));
    let preferred_range = TextRange::new(SourceFileId(99), 10, 40);
    provider.register_method_navigation_ranges(
        "java/util/ArrayList",
        &JavaSourceClassLocation {
            internal_name: "java/util/ArrayList".to_string(),
            declaration_range: SourceByteRange::new(0, 100),
            name_range: SourceByteRange::new(0, 9),
            methods: vec![
                JavaSourceMemberLocation {
                    name: "get".to_string(),
                    descriptor: "(I)Ljava/lang/Object;".to_string(),
                    declaration_range: SourceByteRange::new(10, 40),
                    name_range: SourceByteRange::new(20, 23),
                },
                JavaSourceMemberLocation {
                    name: "get".to_string(),
                    descriptor: "(Ljava/lang/String;)Ljava/lang/String;".to_string(),
                    declaration_range: SourceByteRange::new(50, 90),
                    name_range: SourceByteRange::new(60, 63),
                },
            ],
            fields: Vec::new(),
        },
        SourceFileId(99),
    );
    let collector = collect_with_provider(
        "java_import java.util.ArrayList\n\
             LIST = ArrayList.new\n\
             RESULT = LIST.java_send(:get, [Java::int], 0)\n",
        provider,
    );
    let result =
        FullyQualifiedName::try_from("RESULT").expect("fixture result constant must be valid");
    assert!(collector.analysis().types.iter().any(|fact| {
        fact.subject == TypeSubject::Constant(result.clone())
            && fact.ruby_type
                == RubyType::Class(FullyQualifiedName::try_from("Java::JavaLang::Object").unwrap())
            && fact.provenance == TypeProvenance::Runtime
    }));
    let (selected, selected_range) = collector
        .reference_candidates()
        .iter()
        .find_map(|candidate| match &candidate.kind {
            ReferenceCandidateKind::Method {
                owner,
                owner_kind,
                method,
                preferred_definition_range,
                ..
            } if method.as_str() == "get" => {
                Some(((owner, owner_kind), *preferred_definition_range))
            }
            ReferenceCandidateKind::Constant { .. }
            | ReferenceCandidateKind::Method { .. }
            | ReferenceCandidateKind::Resolved { .. } => None,
        })
        .expect("java_send method-name symbol must reference the selected Java method");
    assert_eq!(
        selected.0.as_slice(),
        FullyQualifiedName::try_from("Java::JavaUtil::ArrayList")
            .unwrap()
            .namespace_parts()
            .as_slice()
    );
    assert_eq!(*selected.1, NamespaceKind::Instance);
    assert_eq!(
        selected_range,
        Some(preferred_range),
        "java_send must retain the exact source/decompiled range for the selected JVM descriptor"
    );
    assert!(collector.diagnostics().is_empty());
}

#[test]
fn decompiled_supplement_retains_only_members_missing_from_exact_source() {
    let exact = JavaSourceClassLocation {
        internal_name: "fixtures/RichFixture".to_string(),
        declaration_range: SourceByteRange::new(0, 100),
        name_range: SourceByteRange::new(6, 17),
        methods: vec![JavaSourceMemberLocation {
            name: "combine".to_string(),
            descriptor: "(Ljava/lang/String;[I)Ljava/util/List;".to_string(),
            declaration_range: SourceByteRange::new(10, 40),
            name_range: SourceByteRange::new(20, 27),
        }],
        fields: vec![JavaSourceMemberLocation {
            name: "COUNT".to_string(),
            descriptor: "I".to_string(),
            declaration_range: SourceByteRange::new(41, 50),
            name_range: SourceByteRange::new(42, 47),
        }],
    };
    let mut decompiled = exact.clone();
    decompiled.methods.push(JavaSourceMemberLocation {
        name: "syntheticBridge".to_string(),
        descriptor: "()V".to_string(),
        declaration_range: SourceByteRange::new(60, 80),
        name_range: SourceByteRange::new(61, 76),
    });
    decompiled.fields.push(JavaSourceMemberLocation {
        name: "GENERATED".to_string(),
        descriptor: "Ljava/lang/String;".to_string(),
        declaration_range: SourceByteRange::new(81, 95),
        name_range: SourceByteRange::new(82, 91),
    });

    let supplemental = supplemental_implementation_location(&exact, decompiled)
        .expect("missing bytecode members must produce a decompiled supplement");
    assert_eq!(
        supplemental
            .methods
            .iter()
            .map(|method| method.name.as_str())
            .collect::<Vec<_>>(),
        vec!["syntheticBridge"]
    );
    assert_eq!(
        supplemental
            .fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec!["GENERATED"]
    );
}

#[test]
fn java_method_distinguishes_bound_static_and_unbound_instance_handles() {
    let mut classes = class_files(&["java/lang/String"]);
    let string = class_mut(&mut classes, "java/lang/String");
    add_methods(
        string,
        [
            MemberInfo {
                access_flags: 0x0009,
                name: "valueOf".into(),
                descriptor: "(I)Ljava/lang/String;".into(),
                exceptions: Box::default(),
                parameters: Box::new([MethodParameter {
                    name: "value".into(),
                }]),
                first_line: None,
            },
            MemberInfo {
                access_flags: 0x0001,
                name: "substring".into(),
                descriptor: "(I)Ljava/lang/String;".into(),
                exceptions: Box::default(),
                parameters: Box::new([MethodParameter {
                    name: "start".into(),
                }]),
                first_line: None,
            },
        ],
    );
    let collector = collect_with_catalog(
        "java_import java.lang.String\n\
             STATIC_HANDLE = String.java_method(:valueOf, [Java::int])\n\
             UNBOUND_HANDLE = String.java_method(:substring, [Java::int])\n\
             INSTANCE = String.new\n\
             BOUND_HANDLE = INSTANCE.java_method(:substring, [Java::int])\n",
        catalog_of(classes),
    );
    for (constant, expected) in [
        ("STATIC_HANDLE", "Method"),
        ("UNBOUND_HANDLE", "UnboundMethod"),
        ("BOUND_HANDLE", "Method"),
    ] {
        let constant = FullyQualifiedName::try_from(constant).unwrap();
        let expected = RubyType::Class(FullyQualifiedName::try_from(expected).unwrap());
        assert!(
            collector.analysis().types.iter().any(|fact| {
                fact.subject == TypeSubject::Constant(constant.clone())
                    && fact.ruby_type == expected
                    && fact.provenance == TypeProvenance::Runtime
            }),
            "{constant} must have type {expected}"
        );
    }
    assert_eq!(
        collector
            .reference_candidates()
            .iter()
            .filter(|candidate| matches!(
                &candidate.kind,
                ReferenceCandidateKind::Method { method, .. }
                    if matches!(method.as_str(), "valueOf" | "substring")
            ))
            .count(),
        3
    );
    assert!(collector.diagnostics().is_empty());
}

#[test]
fn to_java_projects_explicit_object_primitive_and_array_targets() {
    let collector = collect(
        "OBJECT = 'value'.to_java(java.lang.CharSequence)\n\
             INTEGER = 1.to_java(Java::int)\n\
             INTS = [1, 2].to_java(Java::int)\n",
        &["java/lang/CharSequence"],
    );
    for (constant, expected) in [
        (
            "OBJECT",
            RubyType::Class(FullyQualifiedName::try_from("Java::JavaLang::CharSequence").unwrap()),
        ),
        (
            "INTEGER",
            RubyType::Class(FullyQualifiedName::try_from("Java::JavaLang::Integer").unwrap()),
        ),
        ("INTS", RubyType::array_of(RubyType::integer())),
    ] {
        let constant = FullyQualifiedName::try_from(constant).unwrap();
        assert!(
            collector.analysis().types.iter().any(|fact| {
                fact.subject == TypeSubject::Constant(constant.clone())
                    && fact.ruby_type == expected
                    && fact.provenance == TypeProvenance::Runtime
            }),
            "{constant} must have type {expected}"
        );
    }
    assert!(collector.diagnostics().is_empty());
}

#[test]
fn java_interfaces_connect_to_ruby_classes_through_include_and_java_implements() {
    let mut classes = class_files(&["java/lang/Runnable"]);
    class_mut(&mut classes, "java/lang/Runnable").access_flags = 0x0601;
    let collector = collect_with_catalog(
        "class Worker\n\
             \x20 java_implements java.lang.Runnable\n\
             end\n\
             class IncludedWorker\n\
             \x20 include java.lang.Runnable\n\
             end\n",
        catalog_of(classes),
    );
    let target = FullyQualifiedName::namespace(
        ["Java", "JavaLang", "Runnable"]
            .into_iter()
            .map(|part| RubyConstant::new(part).unwrap())
            .collect::<Vec<_>>(),
    );
    for source in ["Worker", "IncludedWorker"] {
        let source = FullyQualifiedName::namespace(vec![RubyConstant::new(source).unwrap()]);
        assert!(collector.analysis().graph_edges.iter().any(|edge| {
            edge.source == source
                && edge.target == target
                && edge.kind == ruby_analysis::core::GraphEdgeKind::Include
        }));
    }
    assert!(collector.diagnostics().is_empty());
}

#[test]
fn ordinary_ruby_include_argument_calls_are_not_java_interfaces() {
    let collector = collect("[301, 302].should include last_response.status\n", &[]);

    assert!(
        collector
            .diagnostics()
            .iter()
            .all(|diagnostic| diagnostic.code != "unresolved-java-interface"),
        "ordinary Ruby include arguments must not be interpreted as JRuby interface names"
    );
}

#[test]
fn java_package_accepts_only_the_static_jrubyc_declaration_form() {
    let collector = collect(
        "java_package 'com.example.generated'\n\
             dynamic_package = 'com.example.dynamic'\n\
             java_package dynamic_package\n",
        &[],
    );
    assert_eq!(collector.diagnostics().len(), 1);
    assert_eq!(
        collector.diagnostics()[0].code,
        "unsupported-jruby-java-package"
    );
}

#[test]
fn supports_string_array_and_nested_java_class_imports() {
    let collector = collect(
        "java_import ['java.lang.String', 'java.util.Map$Entry']\n",
        &["java/lang/String", "java/util/Map$Entry"],
    );
    assert!(collector
        .analysis()
        .symbols
        .iter()
        .any(|fact| fact.fqn == FullyQualifiedName::try_from("String").unwrap()));
    assert!(collector
        .analysis()
        .symbols
        .iter()
        .any(|fact| fact.fqn == FullyQualifiedName::try_from("Entry").unwrap()));
    assert!(collector.analysis().types.iter().any(|fact| {
        fact.ruby_type
            == RubyType::ClassReference(
                FullyQualifiedName::try_from("Java::JavaUtil::Map::Entry").unwrap(),
            )
    }));
}

#[test]
fn evaluates_bounded_java_import_alias_blocks_without_executing_ruby() {
    let collector = collect(
        "module Types\n\
               java_import('java.lang.String') { |package, name| \"J#{name}\" }\n\
             end\n",
        &["java/lang/String"],
    );
    let alias =
        FullyQualifiedName::try_from("Types::JString").expect("fixture alias must be valid");
    assert!(collector
        .analysis()
        .symbols
        .iter()
        .any(|fact| fact.fqn == alias && fact.kind == SymbolKind::Constant));
    assert!(collector.analysis().types.iter().any(|fact| {
        fact.subject == TypeSubject::Constant(alias.clone())
            && fact.ruby_type
                == RubyType::ClassReference(
                    FullyQualifiedName::try_from("Java::JavaLang::String").unwrap(),
                )
    }));
    assert!(collector.diagnostics().is_empty());
}

#[test]
fn reports_missing_project_class_and_dynamic_alias_block() {
    let collector = collect(
        "java_import java.lang.Missing\n\
             java_import(java.lang.String) { |_package, name| name.upcase }\n",
        &["java/lang/String"],
    );
    let codes = collector
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        codes,
        vec!["unresolved-java-import", "unsupported-jruby-import-alias"]
    );
    assert!(collector.analysis().symbols.is_empty());
}

#[test]
fn include_package_and_import_package_add_bounded_lazy_constant_types() {
    let collector = collect(
        "module Util\n  include_package 'java.util'\n  import 'java.lang'\nend\n",
        &[
            "java/util/List",
            "java/util/Map",
            "java/util/Map$Entry",
            "java/lang/String",
        ],
    );
    for (alias, proxy) in [
        ("Util::List", "Java::JavaUtil::List"),
        ("Util::Map", "Java::JavaUtil::Map"),
        ("Util::String", "Java::JavaLang::String"),
    ] {
        assert!(collector.analysis().types.iter().any(|fact| {
            fact.subject == TypeSubject::Constant(FullyQualifiedName::try_from(alias).unwrap())
                && fact.ruby_type
                    == RubyType::ClassReference(FullyQualifiedName::try_from(proxy).unwrap())
        }));
    }
    assert!(
        collector
            .analysis()
            .symbols
            .iter()
            .all(|fact| fact.kind != SymbolKind::Constant),
        "include_package constants are runtime const_missing results, not source declarations"
    );
    assert!(collector.diagnostics().is_empty());
}

#[test]
fn package_preflight_materializes_signatures_but_only_referenced_implementations() {
    let provider = JrubyImportProvider::new(catalog(&[
        "java/util/ArrayList",
        "java/util/HashMap",
        "java/time/Instant",
    ]));
    let plan = provider
        .static_navigation_plan(
            "include_package 'java.util'\n\
                 java_import 'java.time.Instant'\n\
                 LIST = ArrayList.new\n",
        )
        .expect("static navigation plan must resolve checked catalog names");

    assert_eq!(
        plan.signature_class_names,
        vec![
            "java/time/Instant".to_string(),
            "java/util/ArrayList".to_string(),
            "java/util/HashMap".to_string(),
        ],
        "package imports need signatures for every bounded direct class"
    );
    assert_eq!(
        plan.implementation_class_names,
        vec![
            "java/time/Instant".to_string(),
            "java/util/ArrayList".to_string(),
        ],
        "exact source/decompilation work is needed only for explicit imports and referenced \
             package proxies"
    );
}

#[test]
fn call_host_probe_attributes_seed_and_import_handlers() {
    reset_jruby_call_host_probe();
    let before = jruby_call_host_probe_snapshot();
    assert_eq!(before.get(CallHostStat::Entries), 0);

    let provider = Arc::new(JrubyImportProvider::new(catalog(&["java/lang/String"])));
    let _ = collect_with_provider(
        "java_import java.lang.String\n\
             VALUE = Java::JavaLang::String.new\n\
             plain.save\n",
        provider,
    );
    let after = jruby_call_host_probe_snapshot();
    assert!(
        after.get(CallHostStat::Entries) >= 2,
        "java_import and String.new must enter the call host: {after:?}"
    );
    assert!(
        after.get(CallHostStat::Seed) > 0
            && after.get(CallHostStat::Import) > 0
            && after.get(CallHostStat::JavaCtor) > 0,
        "probe must record seed, import, and constructor handler hits: {after:?}"
    );
    assert!(
        after.get(CallHostStat::SeedCatalogHits) >= 1,
        "java.lang.String must count as a seed catalog hit: {after:?}"
    );
    assert!(
        after.get(CallHostStat::JavaCtorInferred) >= 1,
        "String.new on a Java proxy must count as an inferred constructor: {after:?}"
    );
    assert!(
        jruby_call_host_handler_hits(&after) > 0,
        "handler hits must sum to a positive total: {after:?}"
    );
}

#[test]
fn ordinary_ruby_calls_do_not_seed_java_proxy_types() {
    let collector = collect(
        "module Admin\n  user.profile.name\n  plain.save(record)\nend\n",
        &["java/lang/String"],
    );
    assert!(
        collector
            .analysis()
            .types
            .iter()
            .all(|fact| match &fact.ruby_type {
                RubyType::ClassReference(fqn) | RubyType::ModuleReference(fqn) => {
                    !fqn.to_string().contains("Java::")
                }
                RubyType::Class(_)
                | RubyType::Module(_)
                | RubyType::Array(_)
                | RubyType::Hash(_, _)
                | RubyType::Literal(_)
                | RubyType::Shape(_)
                | RubyType::Union(_)
                | RubyType::Unknown => true,
            }),
        "ordinary Ruby call chains must not receive JRuby proxy expression types: {:?}",
        collector.analysis().types
    );
}

#[test]
fn static_navigation_prefilter_covers_every_supported_java_entry_form() {
    let provider = JrubyImportProvider::new(catalog(&["java/lang/String", "com/example/Demo"]));
    for source in [
        "java_import 'java.lang.String'\n",
        "include_package 'java.lang'\nString.new\n",
        "import 'java.lang'\nString.new\n",
        "include java.lang.String\n",
        "java_implements java.lang.String\n",
        "Java::JavaLang::String.new\n",
        "Java :: JavaLang :: String.new\n",
        "com.example.Demo.new\n",
        "com . example . Demo.new\n",
    ] {
        assert!(
            provider.source_may_reference_static_java(source),
            "the exact-catalog prefilter must retain supported Java source: {source:?}"
        );
    }
    assert!(
        !provider.source_may_reference_static_java("module Admin\n  user.profile.name\nend\n"),
        "ordinary Ruby call chains must not pay a redundant JRuby AST traversal"
    );
}
