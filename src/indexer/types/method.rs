use std::fmt;

use regex::Regex;

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct Method(String);

impl From<String> for Method {
    fn from(value: String) -> Self {
        Method(value)
    }
}

impl From<&str> for Method {
    fn from(value: &str) -> Self {
        // Original implementation - panics on invalid method names
        if value.is_empty() {
            panic!("Identifier cannot be empty");
        }

        // regex to validate if the identifier is valid
        let regex = Regex::new(r"^[a-z_][a-z0-9_]*[!?]?$").unwrap();
        if !regex.is_match(value) {
            panic!("Identifier contains invalid characters: {}", value);
        }

        Method(value.to_string())
    }
}

impl Method {
    // A safe alternative to From<&str> that returns a Result instead of panicking
    pub fn safe_from(value: &str) -> Result<Self, String> {
        // validate if the identifier is valid
        if value.is_empty() {
            return Err("Identifier cannot be empty".to_string());
        }

        // regex to validate if the identifier is valid
        let regex = Regex::new(r"^[a-z_][a-z0-9_]*[!?]?$").unwrap();
        if !regex.is_match(value) {
            return Err(format!("Identifier contains invalid characters: {}", value));
        }

        Ok(Method(value.to_string()))
    }
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
