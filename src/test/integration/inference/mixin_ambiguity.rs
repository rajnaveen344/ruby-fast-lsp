use crate::test::harness::check;

#[tokio::test]
async fn test_goto_definition_mixin_ambiguity_no_fallback() {
    // We define:
    // 1. Global services (Object#services)
    // 2. M_A (Module)
    // 3. A (includes M_A, overrides services)
    // 4. B (includes M_A, overrides services)
    // 5. C (unrelated, defines services)
    //
    // We expect goto from M_A#foo to find A#services and B#services, and maybe Global (Object#services).
    // It should NOT find C#services.
    check(
        r#"
def services # Global
end

module M_A
  def foo
    services
#   ^def: services_a, services_b, global_services
  end
end

class A
  include M_A
  def services
#     ^def: services_a
  end
end

class B
  include M_A
  def services
#     ^def: services_b
  end
end

class C
  def services
#     ^def: services_c
  end
end
"#,
    )
    .await;
}
