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
    // Goto from M_A#foo finds the overrides of the classes that include M_A
    // (A#services and B#services). It does not fall back to the global method
    // or to the unrelated C#services.
    check(
        r#"
def services # Global
end

module M_A
  def foo
    services$0
  end
end

class A
  include M_A
  <def>def services
  end</def>
end

class B
  include M_A
  <def>def services
  end</def>
end

class C
  def services
  end
end
"#,
    )
    .await;
}
