#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JavaClassName {
    internal_name: String,
    package: Vec<String>,
    classes: Vec<String>,
}

impl JavaClassName {
    pub fn parse(source: &str) -> Result<Self, JavaNameError> {
        let normalized = source.replace('.', "/");
        let (package, class_name) = normalized
            .rsplit_once('/')
            .ok_or_else(|| JavaNameError::DefaultPackage(source.to_string()))?;
        let package = package
            .split('/')
            .map(|component| validate_java_identifier(component, source))
            .collect::<Result<Vec<_>, _>>()?;
        let classes = class_name
            .split('$')
            .map(|component| validate_java_identifier(component, source))
            .collect::<Result<Vec<_>, _>>()?;
        if classes.is_empty() {
            return Err(JavaNameError::InvalidClassName(source.to_string()));
        }
        Ok(Self {
            internal_name: normalized,
            package,
            classes,
        })
    }

    pub fn internal_name(&self) -> &str {
        &self.internal_name
    }

    pub fn imported_constant(&self) -> &str {
        self.classes
            .last()
            .expect("INVARIANT VIOLATED: validated Java class must have a class component")
    }

    pub fn ruby_namespace_parts(&self) -> Vec<String> {
        let mut parts = vec!["Java".to_string(), self.package_proxy_module()];
        parts.extend(self.classes.iter().cloned());
        parts
    }

    pub fn ruby_fqn(&self) -> String {
        self.ruby_namespace_parts().join("::")
    }

    pub fn package_proxy_module(&self) -> String {
        self.package
            .iter()
            .map(|component| {
                let mut chars = component.chars();
                let first = chars
                    .next()
                    .expect("INVARIANT VIOLATED: validated package component cannot be empty");
                format!("{}{}", first.to_ascii_uppercase(), chars.as_str())
            })
            .collect()
    }

    pub fn package(&self) -> &[String] {
        &self.package
    }

    pub fn classes(&self) -> &[String] {
        &self.classes
    }
}

fn validate_java_identifier(component: &str, source: &str) -> Result<String, JavaNameError> {
    let mut chars = component.chars();
    let Some(first) = chars.next() else {
        return Err(JavaNameError::InvalidClassName(source.to_string()));
    };
    if !(first.is_alphabetic() || first == '_' || first == '$')
        || !chars
            .all(|character| character.is_alphanumeric() || character == '_' || character == '$')
    {
        return Err(JavaNameError::InvalidClassName(source.to_string()));
    }
    Ok(component.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JavaNameError {
    DefaultPackage(String),
    InvalidClassName(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_java_packages_and_nested_classes_to_jruby_proxy_constants() {
        let name = JavaClassName::parse("java.util.Map$Entry")
            .expect("checked Java class name must parse");
        assert_eq!(name.internal_name(), "java/util/Map$Entry");
        assert_eq!(name.imported_constant(), "Entry");
        assert_eq!(name.package_proxy_module(), "JavaUtil");
        assert_eq!(name.ruby_fqn(), "Java::JavaUtil::Map::Entry");

        let time_unit = JavaClassName::parse("java/util/concurrent/TimeUnit")
            .expect("checked Java class name must parse");
        assert_eq!(time_unit.ruby_fqn(), "Java::JavaUtilConcurrent::TimeUnit");
    }

    #[test]
    fn rejects_default_package_and_malformed_names() {
        assert!(matches!(
            JavaClassName::parse("TimeUnit"),
            Err(JavaNameError::DefaultPackage(_))
        ));
        assert!(JavaClassName::parse("java.util.9Invalid").is_err());
    }
}
