use super::*;

#[test]
fn incompatible_extension_index_patches_are_rejected_deterministically() {
    let patch = |extension_id: &str, return_type| {
        IndexPatch::DefineMethod(ruby_fast_lsp_extension_api::DefineMethodPatch {
            name: "factory".to_string(),
            namespace: vec!["Widget".to_string()],
            owner_target: None,
            owner_kind: AbiNamespaceKind::Singleton,
            visibility: ruby_fast_lsp_extension_api::MethodVisibility::Public,
            location: SourceRange {
                start: SourcePosition {
                    line: 1,
                    character: 2,
                },
                end: SourcePosition {
                    line: 1,
                    character: 9,
                },
            },
            params: Vec::new(),
            return_type,
            return_type_source: None,
            source: ruby_fast_lsp_extension_api::PatchSource {
                extension_id: extension_id.to_string(),
                macro_name: "factory".to_string(),
            },
        })
    };
    let left = patch(
        "z-extension",
        Some(ruby_fast_lsp_extension_api::RubyType::Named(
            "String".to_string(),
        )),
    );
    let right = patch(
        "a-extension",
        Some(ruby_fast_lsp_extension_api::RubyType::Named(
            "Integer".to_string(),
        )),
    );

    let err = resolve_index_patch_conflicts(vec![left, right])
        .expect_err("incompatible patches for one semantic identity must be rejected");

    assert_eq!(err.extension_ids, vec!["a-extension", "z-extension"]);
    assert!(err.message.contains("Widget.factory"));
}

#[test]
fn equivalent_extension_index_patches_are_deduplicated() {
    let patch = |extension_id: &str| {
        IndexPatch::ApplyMixin(ruby_fast_lsp_extension_api::ApplyMixinPatch {
            namespace: vec!["Widget".to_string()],
            owner_target: None,
            target_kind: AbiNamespaceKind::Instance,
            mixin_target: None,
            mixin: vec!["Shared".to_string()],
            absolute: true,
            kind: ruby_fast_lsp_extension_api::MixinKind::Include,
            location: SourceRange {
                start: SourcePosition {
                    line: 2,
                    character: 0,
                },
                end: SourcePosition {
                    line: 2,
                    character: 7,
                },
            },
            source: ruby_fast_lsp_extension_api::PatchSource {
                extension_id: extension_id.to_string(),
                macro_name: "shared".to_string(),
            },
        })
    };

    let resolved = resolve_index_patch_conflicts(vec![patch("z-extension"), patch("a-extension")])
        .expect("equivalent semantic patches must merge without ambiguity");

    assert_eq!(resolved.len(), 1);
    assert_eq!(index_patch_extension_id(&resolved[0]), "a-extension");
}

#[test]
fn incompatible_execution_contexts_are_rejected_deterministically() {
    let left = execution_context_fixture("z-extension");
    let mut right = execution_context_fixture("a-extension");
    right.generated_owners[0].owner_kind = AbiNamespaceKind::Singleton;

    let err = resolve_execution_context_conflicts(vec![left, right])
        .expect_err("one block cannot have competing runtime owners");

    assert_eq!(err.extension_ids, vec!["a-extension", "z-extension"]);
    assert!(err.message.contains("block execution contexts"));
}

#[test]
fn incompatible_generated_namespace_kinds_are_rejected_deterministically() {
    let patch = |extension_id: &str, kind| {
        IndexPatch::DefineNamespace(ruby_fast_lsp_extension_api::DefineNamespacePatch {
            namespace: vec!["GeneratedRecord".to_string()],
            kind,
            location: SourceRange {
                start: SourcePosition {
                    line: 1,
                    character: 8,
                },
                end: SourcePosition {
                    line: 1,
                    character: 13,
                },
            },
            source: ruby_fast_lsp_extension_api::PatchSource {
                extension_id: extension_id.to_string(),
                macro_name: "field".to_string(),
            },
        })
    };

    let err = resolve_index_patch_conflicts(vec![
        patch(
            "z-extension",
            ruby_fast_lsp_extension_api::NamespaceDeclarationKind::Class,
        ),
        patch(
            "a-extension",
            ruby_fast_lsp_extension_api::NamespaceDeclarationKind::Module,
        ),
    ])
    .expect_err("class and module declarations for one namespace must conflict");

    assert_eq!(err.extension_ids, vec!["a-extension", "z-extension"]);
    assert!(err.message.contains("GeneratedRecord"));
}

#[test]
fn incompatible_generated_reference_targets_are_rejected_deterministically() {
    let patch = |extension_id: &str, target| {
        IndexPatch::AddReference(ruby_fast_lsp_extension_api::ReferencePatch {
            target,
            location: SourceRange {
                start: SourcePosition {
                    line: 1,
                    character: 8,
                },
                end: SourcePosition {
                    line: 1,
                    character: 13,
                },
            },
            source: ruby_fast_lsp_extension_api::PatchSource {
                extension_id: extension_id.to_string(),
                macro_name: "association".to_string(),
            },
        })
    };

    let err = resolve_index_patch_conflicts(vec![
        patch(
            "z-extension",
            ruby_fast_lsp_extension_api::ReferenceTarget::Namespace(vec!["User".to_string()]),
        ),
        patch(
            "a-extension",
            ruby_fast_lsp_extension_api::ReferenceTarget::Namespace(vec!["Account".to_string()]),
        ),
    ])
    .expect_err("one source range must not resolve to incompatible generated targets");

    assert_eq!(err.extension_ids, vec!["a-extension", "z-extension"]);
    assert!(err.message.contains("reference at 1:8-1:13"));
}

#[test]
fn incompatible_generated_superclasses_are_rejected_deterministically() {
    let patch = |extension_id: &str, superclass: &str| {
        IndexPatch::SetSuperclass(ruby_fast_lsp_extension_api::SetSuperclassPatch {
            namespace: vec!["GeneratedRecord".to_string()],
            superclass: vec![superclass.to_string()],
            absolute: true,
            location: SourceRange {
                start: SourcePosition {
                    line: 1,
                    character: 8,
                },
                end: SourcePosition {
                    line: 1,
                    character: 13,
                },
            },
            source: ruby_fast_lsp_extension_api::PatchSource {
                extension_id: extension_id.to_string(),
                macro_name: "model".to_string(),
            },
        })
    };

    let err = resolve_index_patch_conflicts(vec![
        patch("z-extension", "ApplicationRecord"),
        patch("a-extension", "ActiveRecordBase"),
    ])
    .expect_err("one generated class must not acquire competing superclasses");

    assert_eq!(err.extension_ids, vec!["a-extension", "z-extension"]);
    assert!(err.message.contains("GeneratedRecord superclass"));
}

#[test]
fn invalid_generated_superclass_is_rejected_before_fact_conversion() {
    let patch = IndexPatch::SetSuperclass(ruby_fast_lsp_extension_api::SetSuperclassPatch {
        namespace: vec!["GeneratedRecord".to_string()],
        superclass: Vec::new(),
        absolute: true,
        location: SourceRange {
            start: SourcePosition {
                line: 1,
                character: 8,
            },
            end: SourcePosition {
                line: 1,
                character: 13,
            },
        },
        source: ruby_fast_lsp_extension_api::PatchSource {
            extension_id: "superclass-test".to_string(),
            macro_name: "model".to_string(),
        },
    });

    let err = validate_index_patch_payloads(&[patch])
        .expect_err("empty superclass targets must be rejected at the guest boundary");
    assert!(
        err.contains("superclass target must not be empty"),
        "got: {err}"
    );
}

#[test]
fn runtime_reindex_requests_are_scoped_to_related_workspace_roots() {
    let temp = TempDir::new().expect("runtime reindex temp workspace must be created");
    let root = temp.path().join("workspace");
    let model = root.join("app/models/user.rb");
    fs::create_dir_all(model.parent().expect("model path must have parent"))
        .expect("runtime reindex model directory must be created");
    fs::write(&model, "class User\nend\n").expect("runtime reindex model fixture must be written");
    let root_label = normalized_path(&root);
    let unrelated = temp.path().join("other");
    let requests = vec![ruby_fast_lsp_extension_api::ReindexFile {
        workspace_root: root_label.clone(),
        path: "app/models/user.rb".to_string(),
    }];

    let uris = validate_extension_reindex_files(
        "rails-ruby",
        &[root.clone(), unrelated],
        std::slice::from_ref(&root),
        &requests,
    )
    .expect("a related workspace-relative runtime reindex request must be accepted");
    assert_eq!(uris.len(), 1);
    assert_eq!(
        fs::canonicalize(uris[0].to_file_path().expect("file URI")).expect("model path"),
        fs::canonicalize(model).expect("model fixture must canonicalize")
    );

    let traversal = vec![ruby_fast_lsp_extension_api::ReindexFile {
        workspace_root: root_label,
        path: "../secret.rb".to_string(),
    }];
    let err = validate_extension_reindex_files(
        "rails-ruby",
        std::slice::from_ref(&root),
        std::slice::from_ref(&root),
        &traversal,
    )
    .expect_err("runtime reindex requests must reject parent traversal");
    assert!(err.to_string().contains("workspace-relative"), "got: {err}");
}

#[test]
fn generated_superclass_requires_class_declaration_from_same_guest_output() {
    let patch = IndexPatch::SetSuperclass(ruby_fast_lsp_extension_api::SetSuperclassPatch {
        namespace: vec!["GeneratedRecord".to_string()],
        superclass: vec!["BaseRecord".to_string()],
        absolute: true,
        location: SourceRange {
            start: SourcePosition {
                line: 1,
                character: 8,
            },
            end: SourcePosition {
                line: 1,
                character: 13,
            },
        },
        source: ruby_fast_lsp_extension_api::PatchSource {
            extension_id: "superclass-test".to_string(),
            macro_name: "model".to_string(),
        },
    });

    let err = validate_index_patch_payloads(&[patch]).expect_err(
        "a superclass patch must not override a parser-owned or separately generated class",
    );
    assert!(
        err.contains("requires a matching generated class declaration"),
        "got: {err}"
    );
}

#[test]
fn invalid_generated_reference_target_is_rejected_before_fact_conversion() {
    let patch = IndexPatch::AddReference(ruby_fast_lsp_extension_api::ReferencePatch {
        target: ruby_fast_lsp_extension_api::ReferenceTarget::Namespace(Vec::new()),
        location: SourceRange {
            start: SourcePosition {
                line: 1,
                character: 8,
            },
            end: SourcePosition {
                line: 1,
                character: 13,
            },
        },
        source: ruby_fast_lsp_extension_api::PatchSource {
            extension_id: "reference-test".to_string(),
            macro_name: "association".to_string(),
        },
    });

    let err = validate_index_patch_payloads(&[patch])
        .expect_err("empty generated reference targets must be rejected");
    assert!(err.contains("must not be empty"), "got: {err}");

    let method_patch = IndexPatch::AddReference(ruby_fast_lsp_extension_api::ReferencePatch {
        target: ruby_fast_lsp_extension_api::ReferenceTarget::Method {
            namespace: vec!["User".to_string()],
            owner_kind: AbiNamespaceKind::Instance,
            name: "not a method".to_string(),
        },
        location: SourceRange {
            start: SourcePosition {
                line: 2,
                character: 14,
            },
            end: SourcePosition {
                line: 2,
                character: 26,
            },
        },
        source: ruby_fast_lsp_extension_api::PatchSource {
            extension_id: "reference-test".to_string(),
            macro_name: "before_save".to_string(),
        },
    });
    let err = validate_index_patch_payloads(&[method_patch])
        .expect_err("invalid extension method targets must be rejected");
    assert!(err.contains("invalid reference method name"), "got: {err}");
}

#[test]
fn invalid_generated_constant_metadata_is_rejected_before_fact_conversion() {
    let patch = IndexPatch::DefineConstant(ruby_fast_lsp_extension_api::DefineConstantPatch {
        namespace: vec!["GeneratedRecord".to_string()],
        name: "not-a-constant".to_string(),
        location: SourceRange {
            start: SourcePosition {
                line: 1,
                character: 8,
            },
            end: SourcePosition {
                line: 1,
                character: 13,
            },
        },
        ruby_type: Some(ruby_fast_lsp_extension_api::RubyType::Named(
            "String".to_string(),
        )),
        source: ruby_fast_lsp_extension_api::PatchSource {
            extension_id: "constant-test".to_string(),
            macro_name: "field".to_string(),
        },
    });

    let err = validate_index_patch_payloads(&[patch])
        .expect_err("invalid generated constant names must be rejected at guest boundary");

    assert!(err.contains("invalid constant name"), "got: {err}");
}

#[test]
fn structured_extension_types_are_canonical_and_order_independent() {
    use ruby_fast_lsp_extension_api::RubyType as ExtensionRubyType;

    let left = ExtensionRubyType::Union(vec![
        ExtensionRubyType::Array(vec![ExtensionRubyType::Named("String".to_string())]),
        ExtensionRubyType::Named("NilClass".to_string()),
    ]);
    let right = ExtensionRubyType::Union(vec![
        ExtensionRubyType::Named("NilClass".to_string()),
        ExtensionRubyType::Array(vec![ExtensionRubyType::Named("String".to_string())]),
    ]);

    assert!(extension_ruby_types_semantically_equal(
        Some(&left),
        Some(&right)
    ));
    assert_eq!(
        analysis_ruby_type_from_extension(Some(&left))
            .expect("valid structured type must convert")
            .expect("structured type must produce an analysis type")
            .to_string(),
        "(NilClass | Array<String>)"
    );
}

#[test]
fn malformed_or_excessively_nested_extension_types_are_rejected() {
    use ruby_fast_lsp_extension_api::RubyType as ExtensionRubyType;

    let empty_array = ExtensionRubyType::Array(Vec::new());
    let empty_err = analysis_ruby_type_from_extension(Some(&empty_array))
        .expect_err("empty collection type payloads must be rejected");
    assert!(empty_err.contains("must not be empty"), "got: {empty_err}");

    let mut nested = ExtensionRubyType::Named("String".to_string());
    for _ in 0..10 {
        nested = ExtensionRubyType::Array(vec![nested]);
    }
    let depth_err = analysis_ruby_type_from_extension(Some(&nested))
        .expect_err("deeply nested guest types must be bounded");
    assert!(depth_err.contains("maximum depth"), "got: {depth_err}");
}

#[test]
fn extension_index_patch_provenance_must_match_manifest_identity() {
    let patch = IndexPatch::ApplyMixin(ruby_fast_lsp_extension_api::ApplyMixinPatch {
        namespace: vec!["Widget".to_string()],
        owner_target: None,
        target_kind: AbiNamespaceKind::Instance,
        mixin_target: None,
        mixin: vec!["Shared".to_string()],
        absolute: true,
        kind: ruby_fast_lsp_extension_api::MixinKind::Include,
        location: SourceRange {
            start: SourcePosition {
                line: 0,
                character: 0,
            },
            end: SourcePosition {
                line: 0,
                character: 1,
            },
        },
        source: ruby_fast_lsp_extension_api::PatchSource {
            extension_id: "spoofed-extension".to_string(),
            macro_name: "shared".to_string(),
        },
    });

    let spoofed = validate_index_patch_provenance("loaded-extension", &[patch])
        .expect_err("guest patch provenance must not impersonate another extension");

    assert_eq!(spoofed, "spoofed-extension");
}

#[test]
fn extension_response_patch_provenance_must_match_manifest_identity() {
    let zero = SourcePosition {
        line: 0,
        character: 0,
    };
    let patch = ResponsePatch::DocumentSymbol(ruby_fast_lsp_extension_api::DocumentSymbolPatch {
        name: "Example".to_string(),
        detail: None,
        kind: "Method".to_string(),
        range: SourceRange {
            start: zero,
            end: zero,
        },
        selection_range: SourceRange {
            start: zero,
            end: zero,
        },
        source: ruby_fast_lsp_extension_api::PatchSource {
            extension_id: "spoofed-extension".to_string(),
            macro_name: "symbol".to_string(),
        },
    });

    let spoofed = validate_response_patch_provenance("loaded-extension", &[patch])
        .expect_err("guest response provenance must not impersonate another extension");

    assert_eq!(spoofed, "spoofed-extension");
}

#[test]
fn invalid_extension_method_metadata_is_rejected_before_fact_conversion() {
    let patch = IndexPatch::DefineMethod(ruby_fast_lsp_extension_api::DefineMethodPatch {
        name: "generated".to_string(),
        namespace: vec!["Widget".to_string()],
        owner_target: None,
        owner_kind: AbiNamespaceKind::Instance,
        visibility: ruby_fast_lsp_extension_api::MethodVisibility::Public,
        location: SourceRange {
            start: SourcePosition {
                line: 1,
                character: 0,
            },
            end: SourcePosition {
                line: 1,
                character: 9,
            },
        },
        params: Vec::new(),
        return_type: Some(ruby_fast_lsp_extension_api::RubyType::Named(
            "not a Ruby type".to_string(),
        )),
        return_type_source: None,
        source: ruby_fast_lsp_extension_api::PatchSource {
            extension_id: "metadata-test".to_string(),
            macro_name: "generated".to_string(),
        },
    });

    let err = validate_index_patch_payloads(&[patch.clone()])
        .expect_err("invalid extension return type must be rejected at guest boundary");

    assert!(err.contains("invalid named Ruby type"), "got: {err}");

    let IndexPatch::DefineMethod(mut conflicting_return_source) = patch else {
        unreachable_invariant!(
            what = "the method validation fixture changed patch variants",
            why = "the return-source invariant applies only to DefineMethod",
            fix = "keep this fixture as DefineMethod",
        );
    };
    conflicting_return_source.return_type = Some(ruby_fast_lsp_extension_api::RubyType::Named(
        "Widget".to_string(),
    ));
    conflicting_return_source.return_type_source =
        Some(ruby_fast_lsp_extension_api::MethodReturnTypeSource::Block);
    let err = validate_index_patch_payloads(&[IndexPatch::DefineMethod(conflicting_return_source)])
        .expect_err("a method return must not have both explicit and inferred sources");
    assert!(
        err.contains("either `return_type` or `return_type_source`"),
        "got: {err}"
    );
}

#[test]
fn initialization_option_direct_wasm_in_directory_is_skipped() {
    let temp_dir = TempDir::new().expect("test temp dir must be created");
    fs::write(temp_dir.path().join("extension.wasm"), b"not real wasm")
        .expect("test wasm marker must be written");

    let config = ExtensionLoadConfig {
        package_paths: Vec::new(),
        directory_paths: vec![ConfiguredExtensionPath {
            path: temp_dir.path().to_path_buf(),
            source: ExtensionPathSource::InitializationOptions,
        }],
        project_package_paths: Vec::new(),
        settings: BTreeMap::new(),
    };

    let extensions = load_wasm_extensions(&config);
    invariant!(
        extensions.is_empty(),
        what = "initialization option directory loaded a raw wasm file",
        why = "editor extension directories must contain manifest packages",
        fix = "keep raw wasm loading scoped to environment/dev paths",
    );
}

#[test]
fn invalid_document_symbol_kind_is_recoverable_error() {
    let zero = SourcePosition {
        line: 0,
        character: 0,
    };
    let patch = ResponsePatch::DocumentSymbol(ruby_fast_lsp_extension_api::DocumentSymbolPatch {
        name: "Example".to_string(),
        detail: None,
        kind: "NotASymbolKind".to_string(),
        range: SourceRange {
            start: zero,
            end: zero,
        },
        selection_range: SourceRange {
            start: zero,
            end: zero,
        },
        source: ruby_fast_lsp_extension_api::PatchSource {
            extension_id: "test".to_string(),
            macro_name: "symbol".to_string(),
        },
    });

    let err = response_patch_to_document_symbol(patch)
        .expect_err("invalid symbol kind must be a recoverable extension error");
    invariant!(
        err.contains("unsupported document symbol kind"),
        what = "invalid extension document symbol kind did not produce a clear error",
        why = "extension response patches must disable the extension instead of panicking",
        fix = "keep symbol kind conversion on the recoverable error path",
    );
}
