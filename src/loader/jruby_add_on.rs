//! The JRuby add-on a project holds while its selected runtime is JRuby.
//!
//! The loader builds it from the project's exact classpath and hands it to the
//! owner through [`LoadSink::set_jruby_add_on`](crate::loader::context::LoadSink::set_jruby_add_on).
//! The owner keeps it per project, reads its classpath fingerprint for status,
//! and passes it back to the loader for interactive file passes. Only the
//! loader reaches the import provider behind it.

use crate::environment::runtime::jruby::imports::JrubyImportProvider;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct JrubyAddOn {
    imports: Arc<JrubyImportProvider>,
}

impl JrubyAddOn {
    pub(in crate::loader) fn new(imports: Arc<JrubyImportProvider>) -> Self {
        Self { imports }
    }

    /// SHA-256 of the exact classpath the add-on's Java catalog was built from.
    pub fn classpath_fingerprint(&self) -> &str {
        self.imports.classpath_fingerprint()
    }

    pub(in crate::loader) fn import_provider(&self) -> &Arc<JrubyImportProvider> {
        &self.imports
    }

    /// An add-on over an empty catalog with the given classpath fingerprint.
    #[cfg(test)]
    pub(crate) fn for_classpath_fingerprint(fingerprint: String) -> Self {
        use crate::environment::runtime::jruby::java_catalog::ProjectJavaCatalog;
        Self::new(Arc::new(JrubyImportProvider::new(Arc::new(
            ProjectJavaCatalog::from_test_classes(&fingerprint, Default::default(), Vec::new()),
        ))))
    }
}
