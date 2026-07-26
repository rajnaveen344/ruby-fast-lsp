#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JvmType {
    Byte,
    Char,
    Double,
    Float,
    Int,
    Long,
    Short,
    Boolean,
    Void,
    Object(String),
    Array(Box<JvmType>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodDescriptor {
    pub parameters: Vec<JvmType>,
    pub returns: JvmType,
}

pub fn parse_field_descriptor(descriptor: &str) -> Result<JvmType, &'static str> {
    let mut cursor = DescriptorCursor::new(descriptor);
    let ty = cursor.parse_type(false)?;
    if !cursor.is_finished() {
        return Err("trailing field descriptor bytes");
    }
    Ok(ty)
}

pub fn parse_method_descriptor(descriptor: &str) -> Result<MethodDescriptor, &'static str> {
    let mut cursor = DescriptorCursor::new(descriptor);
    if cursor.take() != Some(b'(') {
        return Err("method descriptor must begin with `(`");
    }
    let mut parameters = Vec::new();
    loop {
        match cursor.peek() {
            Some(b')') => {
                cursor.take();
                break;
            }
            Some(_) => parameters.push(cursor.parse_type(false)?),
            None => return Err("unterminated method parameter descriptor"),
        }
    }
    let returns = cursor.parse_type(true)?;
    if !cursor.is_finished() {
        return Err("trailing method descriptor bytes");
    }
    Ok(MethodDescriptor {
        parameters,
        returns,
    })
}

struct DescriptorCursor<'a> {
    source: &'a str,
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> DescriptorCursor<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            bytes: source.as_bytes(),
            offset: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.offset).copied()
    }

    fn take(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.offset += 1;
        Some(byte)
    }

    fn is_finished(&self) -> bool {
        self.offset == self.bytes.len()
    }

    fn parse_type(&mut self, allow_void: bool) -> Result<JvmType, &'static str> {
        match self.take() {
            Some(b'B') => Ok(JvmType::Byte),
            Some(b'C') => Ok(JvmType::Char),
            Some(b'D') => Ok(JvmType::Double),
            Some(b'F') => Ok(JvmType::Float),
            Some(b'I') => Ok(JvmType::Int),
            Some(b'J') => Ok(JvmType::Long),
            Some(b'S') => Ok(JvmType::Short),
            Some(b'Z') => Ok(JvmType::Boolean),
            Some(b'V') if allow_void => Ok(JvmType::Void),
            Some(b'V') => Err("void is not a field or parameter type"),
            Some(b'L') => {
                let start = self.offset;
                while self.peek().is_some_and(|byte| byte != b';') {
                    self.offset += 1;
                }
                if self.take() != Some(b';') {
                    return Err("unterminated object type");
                }
                let end = self.offset - 1;
                if start == end {
                    return Err("object type name is empty");
                }
                Ok(JvmType::Object(self.source[start..end].to_string()))
            }
            Some(b'[') => {
                let element = self.parse_type(false)?;
                Ok(JvmType::Array(Box::new(element)))
            }
            Some(_) => Err("unknown descriptor type tag"),
            None => Err("missing descriptor type"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_method_descriptor_with_objects_primitives_and_arrays() {
        assert_eq!(
            parse_method_descriptor("(Ljava/lang/String;I[J)Ljava/util/List;"),
            Ok(MethodDescriptor {
                parameters: vec![
                    JvmType::Object("java/lang/String".to_string()),
                    JvmType::Int,
                    JvmType::Array(Box::new(JvmType::Long)),
                ],
                returns: JvmType::Object("java/util/List".to_string()),
            })
        );
    }

    #[test]
    fn rejects_void_field_and_trailing_descriptor_bytes() {
        assert!(parse_field_descriptor("V").is_err());
        assert!(parse_method_descriptor("()Vx").is_err());
    }
}
