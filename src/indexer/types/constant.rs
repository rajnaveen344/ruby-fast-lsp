use std::fmt::{self, Display, Formatter};

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct Constant(String);

impl From<String> for Constant {
    fn from(value: String) -> Self {
        Constant(value)
    }
}

impl From<&str> for Constant {
    fn from(value: &str) -> Self {
        Constant(value.to_string())
    }
}

impl Display for Constant {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
