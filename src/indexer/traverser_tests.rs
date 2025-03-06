#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::Entry;

    #[test]
    fn test_complex_project_indexing() {
        let mut indexer = Indexer::new();

        // Index the complex project files
        let user_file = "tests/fixtures/complex_project/lib/user_management/user.rb";
        let auth_file = "tests/fixtures/complex_project/lib/user_management/authentication.rb";
        let order_file = "tests/fixtures/complex_project/lib/order_processing/order.rb";

        indexer.index_file(user_file.into());
        indexer.index_file(auth_file.into());
        indexer.index_file(order_file.into());

        // Test class definitions
        let user_class = indexer.find_definition("UserManagement::User");
        assert!(user_class.is_some());

        // Test method definitions and references
        let full_name_method = indexer.find_definition("UserManagement::User#full_name");
        assert!(full_name_method.is_some());

        // Test instance variable tracking
        let email_refs = indexer.find_references("@email");
        assert!(email_refs.len() >= 2); // Should find in initialize and validate_email

        // Test module inclusion
        let auth_module = indexer.find_definition("UserManagement::Authentication");
        assert!(auth_module.is_some());

        // Test nested module references
        let order_class = indexer.find_definition("OrderProcessing::Order");
        assert!(order_class.is_some());
    }

    #[test]
    fn test_complex_project_references() {
        let mut indexer = Indexer::new();
        let user_file = "tests/fixtures/complex_project/lib/user_management/user.rb";

        indexer.index_file(user_file.into());

        // Test instance variable references across methods
        let first_name_refs = indexer.find_references("@first_name");
        assert!(first_name_refs.len() >= 2); // Should find in initialize and full_name

        // Test class variable references
        let user_count_refs = indexer.find_references("@@user_count");
        assert!(user_count_refs.len() >= 2); // Should find in initialize and self.count
    }

    #[test]
    fn test_complex_project_reindexing() {
        let mut indexer = Indexer::new();
        let order_file = "tests/fixtures/complex_project/lib/order_processing/order.rb";

        // Initial indexing
        indexer.index_file(order_file.into());
        let initial_refs = indexer.find_references("@items");

        // Reindex the same file
        indexer.index_file(order_file.into());
        let after_reindex_refs = indexer.find_references("@items");

        // References should be consistent after reindexing
        assert_eq!(initial_refs.len(), after_reindex_refs.len());
    }
}
