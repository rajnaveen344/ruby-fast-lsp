use std::fmt::{self, Display, Formatter};
use ustr::Ustr;

const MAX_GENERATED_OWNER_COMPONENT_BYTES: usize = 4096;
const GENERATED_OWNER_PREFIX: &str = "\0ruby-fast-lsp-generated-owner:";

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, PartialOrd, Ord)]
pub struct GeneratedOwnerId(Ustr);

impl GeneratedOwnerId {
    pub fn new(
        extension_id: &str,
        source_identity: &str,
        local_identity: &str,
    ) -> Result<Self, &'static str> {
        for component in [extension_id, source_identity, local_identity] {
            if component.is_empty() {
                return Err("Generated owner identity components cannot be empty");
            }
            if component.len() > MAX_GENERATED_OWNER_COMPONENT_BYTES {
                return Err("Generated owner identity component exceeds 4096 bytes");
            }
        }

        // Length-prefix every component so different triples cannot serialize to
        // the same interned key even when their contents contain separators.
        let key = format!(
            "{GENERATED_OWNER_PREFIX}{}:{}{}:{}{}:{}",
            extension_id.len(),
            extension_id,
            source_identity.len(),
            source_identity,
            local_identity.len(),
            local_identity
        );
        Ok(Self(Ustr::from(&key)))
    }

    pub fn as_str(&self) -> &'static str {
        self.0.as_str()
    }
}

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, PartialOrd, Ord)]
pub struct RubyConstant(Ustr);

impl RubyConstant {
    /// Creates a validated namespace segment.
    /// Returns `Err` if invalid Ruby class/module name.
    pub fn new(name: &str) -> Result<Self, &'static str> {
        if name.is_empty() {
            return Err("Namespace segment cannot be empty");
        }

        let mut chars = name.chars();
        let first = chars.next().unwrap();

        // Must start with uppercase (Unicode-aware)
        if !unicode_ident::is_xid_start(first) || !first.is_uppercase() {
            return Err("Namespace must start with uppercase letter");
        }

        // Subsequent characters must be word-like (letters, numbers, _)
        if !chars.all(unicode_ident::is_xid_continue) {
            return Err("Namespace contains invalid characters");
        }

        Ok(Self(Ustr::from(name)))
    }

    pub fn generated_owner(owner: GeneratedOwnerId) -> Self {
        assert!(
            owner.as_str().starts_with(GENERATED_OWNER_PREFIX),
            "INVARIANT VIOLATED: generated owner identity lacks its reserved prefix. This is a bug because generated semantic owners must never collide with source-level Ruby constants. Fix: construct identities only through GeneratedOwnerId::new."
        );
        Self(owner.0)
    }

    pub(crate) fn from_canonical_generated_owner(identity: &str) -> Result<Self, &'static str> {
        let Some(mut remainder) = identity.strip_prefix(GENERATED_OWNER_PREFIX) else {
            return Err("Generated owner identity lacks its reserved prefix");
        };
        let mut components = Vec::with_capacity(3);
        for _ in 0..3 {
            let Some(colon) = remainder.find(':') else {
                return Err("Generated owner identity lacks a component length delimiter");
            };
            let length = remainder[..colon]
                .parse::<usize>()
                .map_err(|_| "Generated owner identity has an invalid component length")?;
            if length == 0 || length > MAX_GENERATED_OWNER_COMPONENT_BYTES {
                return Err("Generated owner identity component length is out of bounds");
            }
            remainder = &remainder[colon + 1..];
            if length > remainder.len() || !remainder.is_char_boundary(length) {
                return Err("Generated owner identity component length splits its encoded value");
            }
            let (component, tail) = remainder.split_at(length);
            components.push(component);
            remainder = tail;
        }
        if !remainder.is_empty() {
            return Err("Generated owner identity contains trailing bytes");
        }
        let canonical = GeneratedOwnerId::new(components[0], components[1], components[2])?;
        if canonical.as_str() != identity {
            return Err("Generated owner identity is not canonically encoded");
        }
        Ok(Self(canonical.0))
    }

    pub fn is_generated_owner(&self) -> bool {
        self.0.as_str().starts_with(GENERATED_OWNER_PREFIX)
    }

    /// Zero-alloc view into the interned Ustr arena. The returned &str has
    /// 'static lifetime since Ustr stores strings in a global arena.
    pub fn as_str(&self) -> &'static str {
        self.0.as_str()
    }

    /// Splits a "Foo::Bar::Baz" string into validated segments.
    pub fn from_qualified_name(fqn: &str) -> Result<Vec<Self>, &'static str> {
        fqn.split("::")
            .map(|segment| RubyConstant::new(segment.trim()))
            .collect()
    }
}

impl TryFrom<&str> for RubyConstant {
    type Error = &'static str;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        RubyConstant::new(value)
    }
}

impl Display for RubyConstant {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if let Some(identity) = self.0.as_str().strip_prefix(GENERATED_OWNER_PREFIX) {
            write!(f, "#<generated-owner:{identity}>")
        } else {
            write!(f, "{}", self.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let namespace = RubyConstant::new("Foo");
        assert_eq!(namespace.unwrap().to_string(), "Foo");
    }

    #[test]
    fn test_from_qualified_name() {
        let namespaces = RubyConstant::from_qualified_name("Foo::Bar::Baz");
        assert_eq!(namespaces.as_ref().unwrap().len(), 3);
        assert_eq!(namespaces.as_ref().unwrap()[0].to_string(), "Foo");
        assert_eq!(namespaces.as_ref().unwrap()[1].to_string(), "Bar");
        assert_eq!(namespaces.as_ref().unwrap()[2].to_string(), "Baz");
    }

    #[test]
    fn test_try_from() {
        let namespace = RubyConstant::try_from("Foo");
        assert_eq!(namespace.unwrap().to_string(), "Foo");
    }

    #[test]
    fn test_display() {
        let namespace = RubyConstant::new("Foo").unwrap();
        assert_eq!(namespace.to_string(), "Foo");
    }

    #[test]
    fn test_try_from_invalid() {
        let namespace = RubyConstant::try_from("foo");
        assert!(namespace.is_err());
    }

    #[test]
    fn test_try_from_empty() {
        let namespace = RubyConstant::try_from("");
        assert!(namespace.is_err());
    }

    #[test]
    fn generated_owner_is_distinct_from_every_source_constant() {
        let owner = GeneratedOwnerId::new("rspec-ruby", "file:///spec/user_spec.rb", "group:4:2")
            .expect("test generated owner identity must be valid");
        let generated = RubyConstant::generated_owner(owner);
        let named = RubyConstant::new("RspecRubyFileSpecUserSpecRbGroup42")
            .expect("test source constant must be valid");

        assert!(generated.is_generated_owner());
        assert!(!named.is_generated_owner());
        assert_ne!(generated, named);
    }

    #[test]
    fn generated_owner_support_keeps_ruby_constants_compact() {
        assert_eq!(
            std::mem::size_of::<RubyConstant>(),
            std::mem::size_of::<Ustr>(),
            "generated-owner tagging must not increase every indexed namespace segment"
        );
    }

    #[test]
    fn generated_owner_component_boundaries_are_unambiguous() {
        let left = GeneratedOwnerId::new("a", "bc", "d")
            .expect("test generated owner identity must be valid");
        let right = GeneratedOwnerId::new("ab", "c", "d")
            .expect("test generated owner identity must be valid");
        let repeat = GeneratedOwnerId::new("a", "bc", "d")
            .expect("test generated owner identity must be valid");

        assert_ne!(left, right);
        assert_eq!(left, repeat);
    }
}
