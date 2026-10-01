use super::constant_pool::decode_modified_utf8;
use super::*;

fn decode_hex(source: &str) -> Vec<u8> {
    let digits = source
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect::<Vec<_>>();
    assert_eq!(
        digits.len() % 2,
        0,
        "checked hex fixture must contain complete bytes"
    );
    digits
        .chunks_exact(2)
        .map(|pair| {
            let pair = std::str::from_utf8(pair).expect("fixture hex must be ASCII");
            u8::from_str_radix(pair, 16).expect("fixture byte must be valid hex")
        })
        .collect()
}

fn replace_ascii(bytes: &mut [u8], old: &[u8], new: &[u8]) {
    assert_eq!(old.len(), new.len(), "fixture mutation must preserve size");
    let mut replacements = 0;
    for offset in 0..=bytes.len() - old.len() {
        if &bytes[offset..offset + old.len()] == old {
            bytes[offset..offset + old.len()].copy_from_slice(new);
            replacements += 1;
        }
    }
    assert!(replacements > 0, "fixture mutation target must exist");
}

#[test]
fn decodes_jvm_modified_utf8_null_and_supplementary_characters() {
    assert_eq!(
        decode_modified_utf8(&[b'A', 0xc0, 0x80, 0xed, 0xa0, 0xbd, 0xed, 0xb8, 0x80, b'Z']),
        Ok(Some("A\0😀Z".to_string()))
    );
}

#[test]
fn rejects_malformed_modified_utf8_without_lossy_replacement() {
    for malformed in [
        &[0x00][..],
        &[0xc0, 0x81][..],
        &[0xe0, 0x80, 0x80][..],
        &[0xf0, 0x9f, 0x98, 0x80][..],
    ] {
        assert_eq!(
            decode_modified_utf8(malformed),
            Err(MetadataError::InvalidUtf8)
        );
    }
    assert_eq!(
        decode_modified_utf8(&[0xed, 0xa0, 0xbd]),
        Ok(None),
        "a legal unpaired Java UTF-16 surrogate must remain representable as an unused \
             constant-pool entry even though Rust strings cannot contain it"
    );
}

#[test]
fn parses_checked_minimal_class_fixture() {
    let bytes = decode_hex(include_str!("../../fixtures/minimal_class.hex"));
    let class = parse_class(&bytes, ClassLimits::default())
        .expect("checked minimal class fixture must parse");

    assert_eq!(class.major_version, 61);
    assert_eq!(class.name, "com/example/Demo");
    assert_eq!(class.super_name.as_deref(), Some("java/lang/Object"));
    assert_eq!(class.source_file.as_deref(), Some("Demo.java"));
    assert_eq!(class.methods.len(), 1);
    assert_eq!(class.methods[0].access_flags, 0x0001);
    assert_eq!(class.methods[0].name, "<init>");
    assert_eq!(class.methods[0].descriptor, "()V");
}

#[test]
fn default_limits_accept_large_bounded_aggregate_attribute_counts() {
    let mut bytes = decode_hex(include_str!("../../fixtures/minimal_class.hex"));
    let source_file_attribute = decode_hex("00 01 00 08 00 00 00 02 00 09");
    assert!(
        bytes.ends_with(&source_file_attribute),
        "INVARIANT VIOLATED: minimal class fixture shape changed. This is a bug because the \
             aggregate-attribute regression must replace the exact checked class attribute table. \
             Fix: update the regression builder for the new fixture shape."
    );
    bytes.truncate(bytes.len() - source_file_attribute.len());

    let attribute_count = 4_097u16;
    bytes.extend_from_slice(&attribute_count.to_be_bytes());
    for _ in 0..attribute_count {
        bytes.extend_from_slice(&9u16.to_be_bytes());
        bytes.extend_from_slice(&0u32.to_be_bytes());
    }

    let class = parse_class(&bytes, ClassLimits::default())
        .expect("a bounded classfile with a production-sized aggregate attribute count must parse");
    assert_eq!(class.name, "com/example/Demo");
}

#[test]
fn rejects_class_larger_than_configured_bound() {
    let bytes = decode_hex(include_str!("../../fixtures/minimal_class.hex"));
    let mut limits = ClassLimits::default();
    limits.max_class_bytes = bytes.len() - 1;

    assert_eq!(
        parse_class(&bytes, limits),
        Err(MetadataError::LimitExceeded("class bytes"))
    );
}

#[test]
fn rejects_truncated_and_corrupt_classfiles_explicitly() {
    let bytes = decode_hex(include_str!("../../fixtures/rich_fixture.class.hex"));
    assert_eq!(
        parse_class(&bytes[..bytes.len() - 1], ClassLimits::default()),
        Err(MetadataError::Truncated)
    );

    let mut corrupt = bytes;
    corrupt[0] = 0;
    assert_eq!(
        parse_class(&corrupt, ClassLimits::default()),
        Err(MetadataError::InvalidMagic)
    );
}

#[test]
fn enforces_constant_pool_and_attribute_bounds() {
    let bytes = decode_hex(include_str!("../../fixtures/rich_fixture.class.hex"));
    let mut constant_pool_limits = ClassLimits::default();
    constant_pool_limits.max_constant_pool_entries = 1;
    assert_eq!(
        parse_class(&bytes, constant_pool_limits),
        Err(MetadataError::LimitExceeded("constant pool entries"))
    );

    let mut attribute_limits = ClassLimits::default();
    attribute_limits.max_attributes = 1;
    assert_eq!(
        parse_class(&bytes, attribute_limits),
        Err(MetadataError::LimitExceeded("attributes"))
    );
}

#[test]
fn parses_generics_parameters_exceptions_annotations_and_lines() {
    let bytes = decode_hex(include_str!("../../fixtures/rich_fixture.class.hex"));
    let class = parse_class(&bytes, ClassLimits::default()).expect("rich class fixture must parse");

    assert_eq!(class.name, "fixtures/RichFixture");
    assert_eq!(class.interfaces, vec!["java/lang/Runnable"]);
    assert_eq!(
        class.signature.as_deref(),
        Some("<T:Ljava/lang/Number;>Ljava/lang/Object;Ljava/lang/Runnable;")
    );
    assert_eq!(
        class.annotations,
        vec![AnnotationInfo {
            descriptor: "Lfixtures/Marker;".to_string(),
        }]
    );
    assert!(class
        .inner_classes
        .iter()
        .any(|inner| inner.inner_class == "fixtures/RichFixture$Inner"
            && inner.inner_name.as_deref() == Some("Inner")));

    let combine = class
        .methods
        .iter()
        .find(|method| method.name == "combine")
        .expect("rich fixture must contain combine");
    assert_eq!(combine.access_flags & 0x0080, 0x0080);
    assert_eq!(
        combine.signature.as_deref(),
        Some("(Ljava/lang/String;[I)Ljava/util/List<Ljava/lang/String;>;")
    );
    assert_eq!(combine.exceptions, vec!["java/io/IOException"]);
    assert_eq!(
        combine.parameters,
        vec![
            MethodParameter {
                name: "prefix".to_string(),
                access_flags: 0,
            },
            MethodParameter {
                name: "values".to_string(),
                access_flags: 0,
            },
        ]
    );
    assert_eq!(
        combine.annotations,
        vec![AnnotationInfo {
            descriptor: "Lfixtures/Marker;".to_string(),
        }]
    );
    assert_eq!(combine.first_line, Some(24));
}

#[test]
fn classifies_annotation_enum_record_and_inner_class_fixtures() {
    let marker = parse_class(
        &decode_hex(include_str!("../../fixtures/marker.class.hex")),
        ClassLimits::default(),
    )
    .expect("annotation fixture must parse");
    assert_eq!(marker.kind(), ClassKind::Annotation);
    assert_eq!(marker.interfaces, vec!["java/lang/annotation/Annotation"]);
    assert!(marker.methods.iter().any(|method| method.name == "value"));

    let shade = parse_class(
        &decode_hex(include_str!("../../fixtures/shade.class.hex")),
        ClassLimits::default(),
    )
    .expect("enum fixture must parse");
    assert_eq!(shade.kind(), ClassKind::Enum);
    assert_eq!(shade.super_name.as_deref(), Some("java/lang/Enum"));
    assert!(shade
        .fields
        .iter()
        .any(|field| field.name == "RED" && field.access_flags & 0x4000 != 0));

    let point = parse_class(
        &decode_hex(include_str!("../../fixtures/point.class.hex")),
        ClassLimits::default(),
    )
    .expect("record fixture must parse");
    assert_eq!(point.kind(), ClassKind::Record);
    assert_eq!(
        point.record_components,
        vec![
            RecordComponentInfo {
                name: "x".to_string(),
                descriptor: "I".to_string(),
                signature: None,
                annotations: Vec::new(),
            },
            RecordComponentInfo {
                name: "y".to_string(),
                descriptor: "I".to_string(),
                signature: None,
                annotations: Vec::new(),
            },
        ]
    );

    let inner = parse_class(
        &decode_hex(include_str!("../../fixtures/inner.class.hex")),
        ClassLimits::default(),
    )
    .expect("inner class fixture must parse");
    assert_eq!(inner.kind(), ClassKind::Class);
    assert_eq!(inner.name, "fixtures/RichFixture$Inner");
    assert!(inner
        .fields
        .iter()
        .any(|field| field.name == "this$0" && field.access_flags & 0x1000 != 0));
}

#[test]
fn uses_debug_parameter_names_then_deterministic_fallbacks() {
    let mut debug_only = decode_hex(include_str!("../../fixtures/rich_fixture.class.hex"));
    replace_ascii(&mut debug_only, b"MethodParameters", b"IgnoredParamsXXX");
    let class = parse_class(&debug_only, ClassLimits::default())
        .expect("debug-only parameter fixture must parse");
    let combine = class
        .methods
        .iter()
        .find(|method| method.name == "combine")
        .expect("fixture must contain combine");
    assert_eq!(
        combine
            .parameters
            .iter()
            .map(|parameter| parameter.name.as_str())
            .collect::<Vec<_>>(),
        vec!["prefix", "values"]
    );

    replace_ascii(
        &mut debug_only,
        b"LocalVariableTable",
        b"IgnoredLocalAttrXX",
    );
    let class = parse_class(&debug_only, ClassLimits::default())
        .expect("metadata-free parameter fixture must parse");
    let combine = class
        .methods
        .iter()
        .find(|method| method.name == "combine")
        .expect("fixture must contain combine");
    assert_eq!(
        combine
            .parameters
            .iter()
            .map(|parameter| parameter.name.as_str())
            .collect::<Vec<_>>(),
        vec!["arg0", "arg1"]
    );
}

#[test]
fn parses_module_identity_without_loading_the_module() {
    let module = parse_class(
        &decode_hex(include_str!("../../fixtures/module-info.class.hex")),
        ClassLimits::default(),
    )
    .expect("checked module-info fixture must parse");
    assert_eq!(module.kind(), ClassKind::Module);
    assert_eq!(module.name, "module-info");
    assert_eq!(module.module_name.as_deref(), Some("fixtures.sample"));
    assert_eq!(module.source_file.as_deref(), Some("module-info.java"));
}

#[test]
fn retains_overloads_and_member_flags_as_separate_declarations() {
    let class = parse_class(
        &decode_hex(include_str!("../../fixtures/overloads.class.hex")),
        ClassLimits::default(),
    )
    .expect("checked overload fixture must parse");
    assert_eq!(class.kind(), ClassKind::Class);
    assert_eq!(class.access_flags & 0x0400, 0x0400);
    let overloads = class
        .methods
        .iter()
        .filter(|method| method.name == "value")
        .collect::<Vec<_>>();
    assert_eq!(overloads.len(), 2);
    assert_eq!(overloads[0].descriptor, "(I)Ljava/lang/String;");
    assert_eq!(
        overloads[1].descriptor,
        "(Ljava/lang/String;)Ljava/lang/String;"
    );

    let native = class
        .methods
        .iter()
        .find(|method| method.name == "nativeValue")
        .expect("fixture must contain nativeValue");
    assert_eq!(native.visibility(), Visibility::Protected);
    assert!(native.is_static());
    assert!(native.is_native());
    assert!(!native.is_abstract());
}

#[test]
fn every_truncated_prefix_and_bounded_mutation_fails_without_panicking() {
    let bytes = decode_hex(include_str!("../../fixtures/rich_fixture.class.hex"));
    for length in 0..bytes.len() {
        assert!(
            parse_class(&bytes[..length], ClassLimits::default()).is_err(),
            "truncated prefix of {length} bytes must not parse"
        );
    }

    let mut state = 0x8f4d_3b29_u32;
    for _ in 0..256 {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let mut mutated = bytes.clone();
        let offset = usize::try_from(state).expect("u32 must fit usize") % mutated.len();
        mutated[offset] ^= ((state >> 24) as u8) | 1;
        let result = std::panic::catch_unwind(|| {
            let _ = parse_class(&mutated, ClassLimits::default());
        });
        assert!(result.is_ok(), "bounded malformed input must never panic");
    }
}
