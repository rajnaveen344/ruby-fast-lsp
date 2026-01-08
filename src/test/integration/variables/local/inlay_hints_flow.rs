#[cfg(test)]
mod tests {
    use crate::test::harness::check;

    #[tokio::test]
    async fn test_flow_sensitive_local_variable_updates() {
        // b should be Float assigned at line 3, then Integer assigned at line 4
        check(
            r#"
            a<hint label=": Integer"> = 1
            b<hint label=": Float"> = 2.1
            b<hint label=": Integer"> = a
            c<hint label=": Integer"> = b
            "#,
        )
        .await;
    }
}
