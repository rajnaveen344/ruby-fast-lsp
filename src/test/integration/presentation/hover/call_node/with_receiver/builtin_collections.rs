//! Hover on Array and Hash methods typed by RBS and structural Hash evidence.

use crate::test::harness::check;

/// Array methods use RBS types
#[tokio::test]
async fn array_methods() {
    check(
        r#"
arr = [1, 2, 3]
arr.length<hover label="Integer">
"#,
    )
    .await;
}

/// Hash methods use RBS types
#[tokio::test]
async fn hash_methods() {
    check(
        r#"
hash = { a: 1 }
hash.keys<hover label="Array<Symbol>">
"#,
    )
    .await;
}

#[tokio::test]
async fn structural_hash_reads_use_exact_and_dynamic_key_evidence() {
    check(
        r#"
def inspect_payload(dynamic_key)
  payload = { count: 1, user: { name: "Ada" } }
  payload[:count]<hover label="Integer">
  payload[:user][:name]<hover label="String">
  payload[:missing]<hover label="NilClass">
  payload[dynamic_key]<hover label="(Integer | NilClass | { name: String })">
  payload.fetch<hover label="Integer">(:count)
  payload.fetch<hover label="String">(:missing, "fallback")
  payload.dig<hover label="String">(:user, :name)
end
"#,
    )
    .await;
}

#[tokio::test]
async fn structural_hash_iteration_projects_keys_values_and_block_parameters() {
    check(
        r#"
def inspect_payload
  payload = { age: 42, name: "Ada" }
  payload.keys<hover label="Array<Symbol>">
  payload.values<hover label="Array<Integer | String>">
  iterator = payload.each
  iterator<hover label="Enumerator">
  payload.each<hover label="{ age: Integer, name: String }"> do |key, value|
    key<hover label="Symbol">
    value<hover label="(Integer | String)">
  end
end
"#,
    )
    .await;
}
