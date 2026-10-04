//! Constants declared only by the bundled core signatures.

use crate::test::harness::{check, check_multi_file};

#[tokio::test]
async fn gemspec_resolves_rubygems_constants_without_a_runtime() {
    check_multi_file(&[(
        "example.gemspec",
        r#"
<err none>Gem::Specification</err>.new do |spec|
  spec.name = "example"
  spec.version = <err none>Gem::Version</err>.new("1.0")
end
"#,
    )])
    .await;
}

#[tokio::test]
async fn unknown_rubygems_constant_stays_unresolved() {
    check(r#"<err code="unresolved-constant">Gem::NoSuchConstant</err>.new"#).await;
}
