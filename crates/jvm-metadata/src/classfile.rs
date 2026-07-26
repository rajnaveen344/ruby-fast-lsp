#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassLimits {
    pub max_class_bytes: usize,
    pub max_constant_pool_entries: usize,
    pub max_members: usize,
    pub max_attributes: usize,
    pub max_attribute_bytes: usize,
    pub max_annotations: usize,
    pub max_annotation_depth: usize,
}

impl Default for ClassLimits {
    fn default() -> Self {
        Self {
            max_class_bytes: 16 * 1024 * 1024,
            max_constant_pool_entries: 65_535,
            max_members: 65_535,
            max_attributes: 4_096,
            max_attribute_bytes: 8 * 1024 * 1024,
            max_annotations: 4_096,
            max_annotation_depth: 32,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberInfo {
    pub access_flags: u16,
    pub name: String,
    pub descriptor: String,
    pub signature: Option<String>,
    pub exceptions: Vec<String>,
    pub parameters: Vec<MethodParameter>,
    pub annotations: Vec<AnnotationInfo>,
    pub first_line: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Protected,
    Private,
    Package,
}

impl MemberInfo {
    pub fn visibility(&self) -> Visibility {
        if self.access_flags & 0x0001 != 0 {
            Visibility::Public
        } else if self.access_flags & 0x0004 != 0 {
            Visibility::Protected
        } else if self.access_flags & 0x0002 != 0 {
            Visibility::Private
        } else {
            Visibility::Package
        }
    }

    pub fn is_static(&self) -> bool {
        self.access_flags & 0x0008 != 0
    }

    pub fn is_final(&self) -> bool {
        self.access_flags & 0x0010 != 0
    }

    pub fn is_synthetic(&self) -> bool {
        self.access_flags & 0x1000 != 0
    }

    pub fn is_abstract(&self) -> bool {
        self.access_flags & 0x0400 != 0
    }

    pub fn is_native(&self) -> bool {
        self.access_flags & 0x0100 != 0
    }

    pub fn is_varargs(&self) -> bool {
        self.access_flags & 0x0080 != 0
    }

    pub fn is_enum_constant(&self) -> bool {
        self.access_flags & 0x4000 != 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodParameter {
    pub name: String,
    pub access_flags: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotationInfo {
    pub descriptor: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InnerClassInfo {
    pub inner_class: String,
    pub outer_class: Option<String>,
    pub inner_name: Option<String>,
    pub access_flags: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordComponentInfo {
    pub name: String,
    pub descriptor: String,
    pub signature: Option<String>,
    pub annotations: Vec<AnnotationInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassKind {
    Class,
    Interface,
    Annotation,
    Enum,
    Record,
    Module,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassFile {
    pub minor_version: u16,
    pub major_version: u16,
    pub access_flags: u16,
    pub name: String,
    pub super_name: Option<String>,
    pub interfaces: Vec<String>,
    pub fields: Vec<MemberInfo>,
    pub methods: Vec<MemberInfo>,
    pub source_file: Option<String>,
    pub signature: Option<String>,
    pub annotations: Vec<AnnotationInfo>,
    pub inner_classes: Vec<InnerClassInfo>,
    pub record_components: Vec<RecordComponentInfo>,
    pub module_name: Option<String>,
}

impl ClassFile {
    pub fn kind(&self) -> ClassKind {
        if self.access_flags & 0x8000 != 0 {
            ClassKind::Module
        } else if self.access_flags & 0x2000 != 0 {
            ClassKind::Annotation
        } else if self.access_flags & 0x4000 != 0 {
            ClassKind::Enum
        } else if !self.record_components.is_empty() {
            ClassKind::Record
        } else if self.access_flags & 0x0200 != 0 {
            ClassKind::Interface
        } else {
            ClassKind::Class
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetadataError {
    InvalidMagic,
    Truncated,
    LimitExceeded(&'static str),
    InvalidConstantPool(&'static str),
    InvalidIndex(u16),
    InvalidUtf8,
    InvalidAttribute(&'static str),
    DuplicateAttribute(String),
    InvalidDescriptor,
    TrailingBytes,
}

pub fn parse_class(bytes: &[u8], limits: ClassLimits) -> Result<ClassFile, MetadataError> {
    if bytes.len() > limits.max_class_bytes {
        return Err(MetadataError::LimitExceeded("class bytes"));
    }
    let mut parser = ClassParser::new(bytes, limits);
    if parser.cursor.u4()? != 0xcafebabe {
        return Err(MetadataError::InvalidMagic);
    }
    let minor_version = parser.cursor.u2()?;
    let major_version = parser.cursor.u2()?;
    let constant_pool = parser.parse_constant_pool()?;
    let access_flags = parser.cursor.u2()?;
    let this_class = parser.cursor.u2()?;
    let super_class = parser.cursor.u2()?;
    let name = constant_pool.class_name(this_class)?;
    let super_name = if super_class == 0 {
        None
    } else {
        Some(constant_pool.class_name(super_class)?)
    };

    let interface_count = parser.bounded_count("interfaces")?;
    let mut interfaces = Vec::with_capacity(interface_count);
    for _ in 0..interface_count {
        interfaces.push(constant_pool.class_name(parser.cursor.u2()?)?);
    }
    let fields = parser.parse_members(&constant_pool, "fields", MemberKind::Field)?;
    let methods = parser.parse_members(&constant_pool, "methods", MemberKind::Method)?;

    let class_attribute_count = parser.bounded_attribute_count()?;
    let mut source_file = None;
    let mut signature = None;
    let mut annotations = Vec::new();
    let mut inner_classes = Vec::new();
    let mut record_components = Vec::new();
    let mut module_name = None;
    let mut unique_attributes = std::collections::HashSet::new();
    for _ in 0..class_attribute_count {
        let attribute_name = constant_pool.utf8(parser.cursor.u2()?)?;
        let attribute = parser.attribute_bytes()?;
        match attribute_name {
            "SourceFile" => {
                require_unique_attribute(&mut unique_attributes, attribute_name)?;
                let mut attribute_cursor = Cursor::new(attribute);
                source_file = Some(constant_pool.utf8(attribute_cursor.u2()?)?.to_string());
                require_finished(&attribute_cursor)?;
            }
            "Signature" => {
                require_unique_attribute(&mut unique_attributes, attribute_name)?;
                signature = Some(parse_signature_attribute(attribute, &constant_pool)?);
            }
            "RuntimeVisibleAnnotations" | "RuntimeInvisibleAnnotations" => {
                annotations.extend(parser.parse_annotations(attribute, &constant_pool)?);
            }
            "InnerClasses" => {
                require_unique_attribute(&mut unique_attributes, attribute_name)?;
                inner_classes = parser.parse_inner_classes(attribute, &constant_pool)?;
            }
            "Record" => {
                require_unique_attribute(&mut unique_attributes, attribute_name)?;
                record_components = parser.parse_record_components(attribute, &constant_pool)?;
            }
            "Module" => {
                require_unique_attribute(&mut unique_attributes, attribute_name)?;
                module_name = Some(parser.parse_module_name(attribute, &constant_pool)?);
            }
            "BootstrapMethods"
            | "Deprecated"
            | "EnclosingMethod"
            | "NestHost"
            | "NestMembers"
            | "PermittedSubclasses"
            | "RuntimeVisibleTypeAnnotations"
            | "RuntimeInvisibleTypeAnnotations"
            | "Synthetic" => {}
            _ => {}
        }
    }
    if !parser.cursor.is_finished() {
        return Err(MetadataError::TrailingBytes);
    }

    Ok(ClassFile {
        minor_version,
        major_version,
        access_flags,
        name,
        super_name,
        interfaces,
        fields,
        methods,
        source_file,
        signature,
        annotations,
        inner_classes,
        record_components,
        module_name,
    })
}

struct ClassParser<'a> {
    cursor: Cursor<'a>,
    limits: ClassLimits,
    attributes_seen: usize,
}

impl<'a> ClassParser<'a> {
    fn new(bytes: &'a [u8], limits: ClassLimits) -> Self {
        Self {
            cursor: Cursor::new(bytes),
            limits,
            attributes_seen: 0,
        }
    }

    fn parse_constant_pool(&mut self) -> Result<ConstantPool, MetadataError> {
        let count = usize::from(self.cursor.u2()?);
        if count == 0 {
            return Err(MetadataError::InvalidConstantPool(
                "constant pool count is zero",
            ));
        }
        if count > self.limits.max_constant_pool_entries {
            return Err(MetadataError::LimitExceeded("constant pool entries"));
        }
        let mut entries = Vec::with_capacity(count);
        entries.push(ConstantPoolEntry::Unusable);
        let mut index = 1usize;
        while index < count {
            let tag = self.cursor.u1()?;
            let entry = match tag {
                1 => {
                    let length = usize::from(self.cursor.u2()?);
                    let bytes = self.cursor.take(length)?;
                    let value = decode_modified_utf8(bytes)?;
                    ConstantPoolEntry::Utf8(value)
                }
                3 | 4 => {
                    self.cursor.take(4)?;
                    ConstantPoolEntry::Other
                }
                5 | 6 => {
                    self.cursor.take(8)?;
                    entries.push(ConstantPoolEntry::Other);
                    index += 1;
                    ConstantPoolEntry::Other
                }
                7 => ConstantPoolEntry::Class(self.cursor.u2()?),
                19 => ConstantPoolEntry::Module(self.cursor.u2()?),
                8 | 16 | 20 => {
                    self.cursor.take(2)?;
                    ConstantPoolEntry::Other
                }
                9 | 10 | 11 | 12 | 17 | 18 => {
                    self.cursor.take(4)?;
                    ConstantPoolEntry::Other
                }
                15 => {
                    self.cursor.take(3)?;
                    ConstantPoolEntry::Other
                }
                _ => {
                    return Err(MetadataError::InvalidConstantPool(
                        "unknown constant pool tag",
                    ))
                }
            };
            entries.push(entry);
            index += 1;
        }
        if entries.len() != count {
            return Err(MetadataError::InvalidConstantPool(
                "wide constant pool entry exceeds declared count",
            ));
        }
        Ok(ConstantPool { entries })
    }

    fn parse_members(
        &mut self,
        constant_pool: &ConstantPool,
        limit_name: &'static str,
        kind: MemberKind,
    ) -> Result<Vec<MemberInfo>, MetadataError> {
        let count = self.bounded_count(limit_name)?;
        let mut members = Vec::with_capacity(count);
        for _ in 0..count {
            let access_flags = self.cursor.u2()?;
            let name = constant_pool.utf8(self.cursor.u2()?)?.to_string();
            let descriptor = constant_pool.utf8(self.cursor.u2()?)?.to_string();
            match kind {
                MemberKind::Field => {
                    crate::descriptor::parse_field_descriptor(&descriptor)
                        .map_err(|_| MetadataError::InvalidDescriptor)?;
                }
                MemberKind::Method => {
                    crate::descriptor::parse_method_descriptor(&descriptor)
                        .map_err(|_| MetadataError::InvalidDescriptor)?;
                }
            }
            let attribute_count = self.bounded_attribute_count()?;
            let mut signature = None;
            let mut exceptions = Vec::new();
            let mut parameters = Vec::new();
            let mut annotations = Vec::new();
            let mut first_line = None;
            let mut local_variables = Vec::new();
            let mut unique_attributes = std::collections::HashSet::new();
            for _ in 0..attribute_count {
                let attribute_name = constant_pool.utf8(self.cursor.u2()?)?;
                let attribute = self.attribute_bytes()?;
                match attribute_name {
                    "Signature" => {
                        require_unique_attribute(&mut unique_attributes, attribute_name)?;
                        signature = Some(parse_signature_attribute(attribute, constant_pool)?);
                    }
                    "Exceptions" => {
                        require_unique_attribute(&mut unique_attributes, attribute_name)?;
                        exceptions = self.parse_exceptions(attribute, constant_pool)?;
                    }
                    "MethodParameters" => {
                        require_unique_attribute(&mut unique_attributes, attribute_name)?;
                        parameters = self.parse_method_parameters(attribute, constant_pool)?;
                    }
                    "RuntimeVisibleAnnotations" | "RuntimeInvisibleAnnotations" => {
                        annotations.extend(self.parse_annotations(attribute, constant_pool)?);
                    }
                    "Code" => {
                        require_unique_attribute(&mut unique_attributes, attribute_name)?;
                        let code = self.parse_code_metadata(attribute, constant_pool)?;
                        first_line = code.first_line;
                        local_variables = code.local_variables;
                    }
                    "AnnotationDefault"
                    | "ConstantValue"
                    | "Deprecated"
                    | "RuntimeVisibleParameterAnnotations"
                    | "RuntimeInvisibleParameterAnnotations"
                    | "RuntimeVisibleTypeAnnotations"
                    | "RuntimeInvisibleTypeAnnotations"
                    | "Synthetic" => {}
                    _ => {}
                }
            }
            if kind == MemberKind::Method {
                let method_descriptor = crate::descriptor::parse_method_descriptor(&descriptor)
                    .map_err(|_| MetadataError::InvalidDescriptor)?;
                if parameters.len() > method_descriptor.parameters.len() {
                    return Err(MetadataError::InvalidAttribute(
                        "MethodParameters exceeds descriptor arity",
                    ));
                }
                let mut slot = if access_flags & 0x0008 == 0 { 1 } else { 0 };
                for (index, parameter_type) in method_descriptor.parameters.iter().enumerate() {
                    if index >= parameters.len() {
                        let debug_name = local_variables
                            .iter()
                            .filter(|local| local.slot == slot)
                            .min_by_key(|local| local.start_pc)
                            .filter(|local| local.start_pc == 0)
                            .map(|local| local.name.clone());
                        parameters.push(MethodParameter {
                            name: debug_name.unwrap_or_else(|| format!("arg{index}")),
                            access_flags: 0,
                        });
                    }
                    slot += match parameter_type {
                        crate::descriptor::JvmType::Long | crate::descriptor::JvmType::Double => 2,
                        crate::descriptor::JvmType::Byte
                        | crate::descriptor::JvmType::Char
                        | crate::descriptor::JvmType::Float
                        | crate::descriptor::JvmType::Int
                        | crate::descriptor::JvmType::Short
                        | crate::descriptor::JvmType::Boolean
                        | crate::descriptor::JvmType::Object(_)
                        | crate::descriptor::JvmType::Array(_) => 1,
                        crate::descriptor::JvmType::Void => {
                            return Err(MetadataError::InvalidDescriptor)
                        }
                    };
                }
                for index in parameters.len()..method_descriptor.parameters.len() {
                    parameters.push(MethodParameter {
                        name: format!("arg{index}"),
                        access_flags: 0,
                    });
                }
            }
            members.push(MemberInfo {
                access_flags,
                name,
                descriptor,
                signature,
                exceptions,
                parameters,
                annotations,
                first_line,
            });
        }
        Ok(members)
    }

    fn bounded_count(&mut self, name: &'static str) -> Result<usize, MetadataError> {
        let count = usize::from(self.cursor.u2()?);
        if count > self.limits.max_members {
            return Err(MetadataError::LimitExceeded(name));
        }
        Ok(count)
    }

    fn bounded_attribute_count(&mut self) -> Result<usize, MetadataError> {
        let count = usize::from(self.cursor.u2()?);
        self.record_attribute_count(count)?;
        Ok(count)
    }

    fn bounded_attribute_count_from(
        &mut self,
        cursor: &mut Cursor<'_>,
    ) -> Result<usize, MetadataError> {
        let count = usize::from(cursor.u2()?);
        self.record_attribute_count(count)?;
        Ok(count)
    }

    fn record_attribute_count(&mut self, count: usize) -> Result<(), MetadataError> {
        self.attributes_seen = self
            .attributes_seen
            .checked_add(count)
            .ok_or(MetadataError::LimitExceeded("attributes"))?;
        if self.attributes_seen > self.limits.max_attributes {
            return Err(MetadataError::LimitExceeded("attributes"));
        }
        Ok(())
    }

    fn attribute_bytes(&mut self) -> Result<&'a [u8], MetadataError> {
        let length = usize::try_from(self.cursor.u4()?)
            .map_err(|_| MetadataError::LimitExceeded("attribute bytes"))?;
        if length > self.limits.max_attribute_bytes {
            return Err(MetadataError::LimitExceeded("attribute bytes"));
        }
        self.cursor.take(length)
    }

    fn parse_exceptions(
        &self,
        bytes: &[u8],
        constant_pool: &ConstantPool,
    ) -> Result<Vec<String>, MetadataError> {
        let mut cursor = Cursor::new(bytes);
        let count = bounded_nested_count(&mut cursor, self.limits.max_members, "exceptions")?;
        let mut exceptions = Vec::with_capacity(count);
        for _ in 0..count {
            exceptions.push(constant_pool.class_name(cursor.u2()?)?);
        }
        require_finished(&cursor)?;
        Ok(exceptions)
    }

    fn parse_method_parameters(
        &self,
        bytes: &[u8],
        constant_pool: &ConstantPool,
    ) -> Result<Vec<MethodParameter>, MetadataError> {
        let mut cursor = Cursor::new(bytes);
        let count = usize::from(cursor.u1()?);
        if count > self.limits.max_members {
            return Err(MetadataError::LimitExceeded("method parameters"));
        }
        let mut parameters = Vec::with_capacity(count);
        for index in 0..count {
            let name_index = cursor.u2()?;
            let name = if name_index == 0 {
                format!("arg{index}")
            } else {
                constant_pool.utf8(name_index)?.to_string()
            };
            parameters.push(MethodParameter {
                name,
                access_flags: cursor.u2()?,
            });
        }
        require_finished(&cursor)?;
        Ok(parameters)
    }

    fn parse_annotations(
        &self,
        bytes: &[u8],
        constant_pool: &ConstantPool,
    ) -> Result<Vec<AnnotationInfo>, MetadataError> {
        let mut cursor = Cursor::new(bytes);
        let count = bounded_nested_count(&mut cursor, self.limits.max_annotations, "annotations")?;
        let mut annotations = Vec::with_capacity(count);
        for _ in 0..count {
            annotations.push(parse_annotation(
                &mut cursor,
                constant_pool,
                self.limits,
                0,
            )?);
        }
        require_finished(&cursor)?;
        Ok(annotations)
    }

    fn parse_code_metadata(
        &mut self,
        bytes: &[u8],
        constant_pool: &ConstantPool,
    ) -> Result<CodeMetadata, MetadataError> {
        let mut cursor = Cursor::new(bytes);
        cursor.u2()?;
        cursor.u2()?;
        let code_length = usize::try_from(cursor.u4()?)
            .map_err(|_| MetadataError::LimitExceeded("code bytes"))?;
        if code_length > self.limits.max_attribute_bytes {
            return Err(MetadataError::LimitExceeded("code bytes"));
        }
        cursor.take(code_length)?;
        let exception_count =
            bounded_nested_count(&mut cursor, self.limits.max_members, "code exceptions")?;
        cursor.take(
            exception_count
                .checked_mul(8)
                .ok_or(MetadataError::LimitExceeded("code exceptions"))?,
        )?;
        let attribute_count = self.bounded_attribute_count_from(&mut cursor)?;
        let mut first_line = None;
        let mut local_variables = Vec::new();
        let mut saw_line_table = false;
        let mut saw_local_variable_table = false;
        for _ in 0..attribute_count {
            let name = constant_pool.utf8(cursor.u2()?)?;
            let length = usize::try_from(cursor.u4()?)
                .map_err(|_| MetadataError::LimitExceeded("attribute bytes"))?;
            if length > self.limits.max_attribute_bytes {
                return Err(MetadataError::LimitExceeded("attribute bytes"));
            }
            let attribute = cursor.take(length)?;
            if name == "LineNumberTable" {
                if saw_line_table {
                    return Err(MetadataError::DuplicateAttribute(name.to_string()));
                }
                saw_line_table = true;
                first_line = parse_first_line(attribute, self.limits)?;
            } else if name == "LocalVariableTable" {
                if saw_local_variable_table {
                    return Err(MetadataError::DuplicateAttribute(name.to_string()));
                }
                saw_local_variable_table = true;
                local_variables = parse_local_variables(attribute, constant_pool, self.limits)?;
            }
        }
        require_finished(&cursor)?;
        Ok(CodeMetadata {
            first_line,
            local_variables,
        })
    }

    fn parse_inner_classes(
        &self,
        bytes: &[u8],
        constant_pool: &ConstantPool,
    ) -> Result<Vec<InnerClassInfo>, MetadataError> {
        let mut cursor = Cursor::new(bytes);
        let count =
            bounded_nested_count(&mut cursor, self.limits.max_members, "inner class entries")?;
        let mut classes = Vec::with_capacity(count);
        for _ in 0..count {
            let inner_index = cursor.u2()?;
            if inner_index == 0 {
                return Err(MetadataError::InvalidAttribute(
                    "InnerClasses entry has zero inner class",
                ));
            }
            let outer_index = cursor.u2()?;
            let name_index = cursor.u2()?;
            classes.push(InnerClassInfo {
                inner_class: constant_pool.class_name(inner_index)?,
                outer_class: if outer_index == 0 {
                    None
                } else {
                    Some(constant_pool.class_name(outer_index)?)
                },
                inner_name: if name_index == 0 {
                    None
                } else {
                    Some(constant_pool.utf8(name_index)?.to_string())
                },
                access_flags: cursor.u2()?,
            });
        }
        require_finished(&cursor)?;
        Ok(classes)
    }

    fn parse_record_components(
        &mut self,
        bytes: &[u8],
        constant_pool: &ConstantPool,
    ) -> Result<Vec<RecordComponentInfo>, MetadataError> {
        let mut cursor = Cursor::new(bytes);
        let count =
            bounded_nested_count(&mut cursor, self.limits.max_members, "record components")?;
        let mut components = Vec::with_capacity(count);
        for _ in 0..count {
            let name = constant_pool.utf8(cursor.u2()?)?.to_string();
            let descriptor = constant_pool.utf8(cursor.u2()?)?.to_string();
            crate::descriptor::parse_field_descriptor(&descriptor)
                .map_err(|_| MetadataError::InvalidDescriptor)?;
            let attribute_count = self.bounded_attribute_count_from(&mut cursor)?;
            let mut signature = None;
            let mut annotations = Vec::new();
            let mut unique_attributes = std::collections::HashSet::new();
            for _ in 0..attribute_count {
                let attribute_name = constant_pool.utf8(cursor.u2()?)?;
                let length = usize::try_from(cursor.u4()?)
                    .map_err(|_| MetadataError::LimitExceeded("attribute bytes"))?;
                if length > self.limits.max_attribute_bytes {
                    return Err(MetadataError::LimitExceeded("attribute bytes"));
                }
                let attribute = cursor.take(length)?;
                match attribute_name {
                    "Signature" => {
                        require_unique_attribute(&mut unique_attributes, attribute_name)?;
                        signature = Some(parse_signature_attribute(attribute, constant_pool)?);
                    }
                    "RuntimeVisibleAnnotations" | "RuntimeInvisibleAnnotations" => {
                        annotations.extend(self.parse_annotations(attribute, constant_pool)?);
                    }
                    "RuntimeVisibleTypeAnnotations" | "RuntimeInvisibleTypeAnnotations" => {}
                    _ => {}
                }
            }
            components.push(RecordComponentInfo {
                name,
                descriptor,
                signature,
                annotations,
            });
        }
        require_finished(&cursor)?;
        Ok(components)
    }

    fn parse_module_name(
        &self,
        bytes: &[u8],
        constant_pool: &ConstantPool,
    ) -> Result<String, MetadataError> {
        let mut cursor = Cursor::new(bytes);
        let module_index = cursor.u2()?;
        let name = constant_pool.module_name(module_index)?.to_string();
        cursor.u2()?;
        let version_index = cursor.u2()?;
        if version_index != 0 {
            constant_pool.utf8(version_index)?;
        }
        let requires_count =
            bounded_nested_count(&mut cursor, self.limits.max_members, "module requires")?;
        for _ in 0..requires_count {
            cursor.u2()?;
            cursor.u2()?;
            let requires_version_index = cursor.u2()?;
            if requires_version_index != 0 {
                constant_pool.utf8(requires_version_index)?;
            }
        }
        parse_module_exports_or_opens(&mut cursor, self.limits.max_members, "module exports")?;
        parse_module_exports_or_opens(&mut cursor, self.limits.max_members, "module opens")?;
        let uses_count = bounded_nested_count(&mut cursor, self.limits.max_members, "module uses")?;
        for _ in 0..uses_count {
            cursor.u2()?;
        }
        let provides_count =
            bounded_nested_count(&mut cursor, self.limits.max_members, "module provides")?;
        for _ in 0..provides_count {
            cursor.u2()?;
            let implementation_count = bounded_nested_count(
                &mut cursor,
                self.limits.max_members,
                "module implementations",
            )?;
            for _ in 0..implementation_count {
                cursor.u2()?;
            }
        }
        require_finished(&cursor)?;
        Ok(name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemberKind {
    Field,
    Method,
}

struct CodeMetadata {
    first_line: Option<u16>,
    local_variables: Vec<LocalVariableInfo>,
}

struct LocalVariableInfo {
    start_pc: u16,
    slot: usize,
    name: String,
}

fn require_unique_attribute(
    seen: &mut std::collections::HashSet<String>,
    name: &str,
) -> Result<(), MetadataError> {
    if !seen.insert(name.to_string()) {
        return Err(MetadataError::DuplicateAttribute(name.to_string()));
    }
    Ok(())
}

fn require_finished(cursor: &Cursor<'_>) -> Result<(), MetadataError> {
    if !cursor.is_finished() {
        return Err(MetadataError::InvalidAttribute(
            "attribute contains trailing bytes",
        ));
    }
    Ok(())
}

fn parse_signature_attribute(
    bytes: &[u8],
    constant_pool: &ConstantPool,
) -> Result<String, MetadataError> {
    let mut cursor = Cursor::new(bytes);
    let signature = constant_pool.utf8(cursor.u2()?)?.to_string();
    require_finished(&cursor)?;
    Ok(signature)
}

fn bounded_nested_count(
    cursor: &mut Cursor<'_>,
    maximum: usize,
    name: &'static str,
) -> Result<usize, MetadataError> {
    let count = usize::from(cursor.u2()?);
    if count > maximum {
        return Err(MetadataError::LimitExceeded(name));
    }
    Ok(count)
}

fn parse_annotation(
    cursor: &mut Cursor<'_>,
    constant_pool: &ConstantPool,
    limits: ClassLimits,
    depth: usize,
) -> Result<AnnotationInfo, MetadataError> {
    if depth > limits.max_annotation_depth {
        return Err(MetadataError::LimitExceeded("annotation depth"));
    }
    let descriptor = constant_pool.utf8(cursor.u2()?)?.to_string();
    crate::descriptor::parse_field_descriptor(&descriptor)
        .map_err(|_| MetadataError::InvalidDescriptor)?;
    let pair_count =
        bounded_nested_count(cursor, limits.max_annotations, "annotation element pairs")?;
    for _ in 0..pair_count {
        constant_pool.utf8(cursor.u2()?)?;
        skip_annotation_value(cursor, constant_pool, limits, depth)?;
    }
    Ok(AnnotationInfo { descriptor })
}

fn skip_annotation_value(
    cursor: &mut Cursor<'_>,
    constant_pool: &ConstantPool,
    limits: ClassLimits,
    depth: usize,
) -> Result<(), MetadataError> {
    match cursor.u1()? {
        b'B' | b'C' | b'D' | b'F' | b'I' | b'J' | b'S' | b'Z' | b's' | b'c' => {
            cursor.u2()?;
        }
        b'e' => {
            cursor.u2()?;
            cursor.u2()?;
        }
        b'@' => {
            parse_annotation(cursor, constant_pool, limits, depth + 1)?;
        }
        b'[' => {
            let count =
                bounded_nested_count(cursor, limits.max_annotations, "annotation array values")?;
            for _ in 0..count {
                skip_annotation_value(cursor, constant_pool, limits, depth + 1)?;
            }
        }
        _ => {
            return Err(MetadataError::InvalidAttribute(
                "unknown annotation element value tag",
            ))
        }
    }
    Ok(())
}

fn parse_first_line(bytes: &[u8], limits: ClassLimits) -> Result<Option<u16>, MetadataError> {
    let mut cursor = Cursor::new(bytes);
    let count = bounded_nested_count(&mut cursor, limits.max_members, "line number table entries")?;
    let mut first_line = None;
    for _ in 0..count {
        cursor.u2()?;
        let line = cursor.u2()?;
        first_line = Some(first_line.map_or(line, |current: u16| current.min(line)));
    }
    require_finished(&cursor)?;
    Ok(first_line)
}

fn parse_local_variables(
    bytes: &[u8],
    constant_pool: &ConstantPool,
    limits: ClassLimits,
) -> Result<Vec<LocalVariableInfo>, MetadataError> {
    let mut cursor = Cursor::new(bytes);
    let count = bounded_nested_count(
        &mut cursor,
        limits.max_members,
        "local variable table entries",
    )?;
    let mut locals = Vec::with_capacity(count);
    for _ in 0..count {
        let start_pc = cursor.u2()?;
        cursor.u2()?;
        let name = constant_pool.utf8(cursor.u2()?)?.to_string();
        let descriptor = constant_pool.utf8(cursor.u2()?)?;
        crate::descriptor::parse_field_descriptor(descriptor)
            .map_err(|_| MetadataError::InvalidDescriptor)?;
        locals.push(LocalVariableInfo {
            start_pc,
            slot: usize::from(cursor.u2()?),
            name,
        });
    }
    require_finished(&cursor)?;
    Ok(locals)
}

fn parse_module_exports_or_opens(
    cursor: &mut Cursor<'_>,
    maximum: usize,
    name: &'static str,
) -> Result<(), MetadataError> {
    let count = bounded_nested_count(cursor, maximum, name)?;
    for _ in 0..count {
        cursor.u2()?;
        cursor.u2()?;
        let target_count = bounded_nested_count(cursor, maximum, name)?;
        for _ in 0..target_count {
            cursor.u2()?;
        }
    }
    Ok(())
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn u1(&mut self) -> Result<u8, MetadataError> {
        Ok(self.take(1)?[0])
    }

    fn u2(&mut self) -> Result<u16, MetadataError> {
        let bytes = self.take(2)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    fn u4(&mut self) -> Result<u32, MetadataError> {
        let bytes = self.take(4)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], MetadataError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(MetadataError::Truncated)?;
        let result = self
            .bytes
            .get(self.offset..end)
            .ok_or(MetadataError::Truncated)?;
        self.offset = end;
        Ok(result)
    }

    fn is_finished(&self) -> bool {
        self.offset == self.bytes.len()
    }
}

enum ConstantPoolEntry {
    Unusable,
    Utf8(Option<String>),
    Class(u16),
    Module(u16),
    Other,
}

struct ConstantPool {
    entries: Vec<ConstantPoolEntry>,
}

impl ConstantPool {
    fn entry(&self, index: u16) -> Result<&ConstantPoolEntry, MetadataError> {
        if index == 0 {
            return Err(MetadataError::InvalidIndex(index));
        }
        self.entries
            .get(usize::from(index))
            .ok_or(MetadataError::InvalidIndex(index))
    }

    fn utf8(&self, index: u16) -> Result<&str, MetadataError> {
        match self.entry(index)? {
            ConstantPoolEntry::Utf8(Some(value)) => Ok(value),
            ConstantPoolEntry::Utf8(None) => Err(MetadataError::InvalidUtf8),
            ConstantPoolEntry::Unusable
            | ConstantPoolEntry::Class(_)
            | ConstantPoolEntry::Module(_)
            | ConstantPoolEntry::Other => Err(MetadataError::InvalidConstantPool(
                "constant pool entry is not Utf8",
            )),
        }
    }

    fn class_name(&self, index: u16) -> Result<String, MetadataError> {
        match self.entry(index)? {
            ConstantPoolEntry::Class(name_index) => Ok(self.utf8(*name_index)?.to_string()),
            ConstantPoolEntry::Unusable
            | ConstantPoolEntry::Utf8(_)
            | ConstantPoolEntry::Module(_)
            | ConstantPoolEntry::Other => Err(MetadataError::InvalidConstantPool(
                "constant pool entry is not Class",
            )),
        }
    }

    fn module_name(&self, index: u16) -> Result<&str, MetadataError> {
        match self.entry(index)? {
            ConstantPoolEntry::Module(name_index) => self.utf8(*name_index),
            ConstantPoolEntry::Unusable
            | ConstantPoolEntry::Utf8(_)
            | ConstantPoolEntry::Class(_)
            | ConstantPoolEntry::Other => Err(MetadataError::InvalidConstantPool(
                "constant pool entry is not Module",
            )),
        }
    }
}

fn decode_modified_utf8(bytes: &[u8]) -> Result<Option<String>, MetadataError> {
    let mut utf16 = Vec::with_capacity(bytes.len());
    let mut offset = 0usize;
    while offset < bytes.len() {
        let first = bytes[offset];
        match first {
            0x01..=0x7f => {
                utf16.push(u16::from(first));
                offset += 1;
            }
            0xc0 => {
                if bytes.get(offset + 1) != Some(&0x80) {
                    return Err(MetadataError::InvalidUtf8);
                }
                utf16.push(0);
                offset += 2;
            }
            0xc2..=0xdf => {
                let second = *bytes.get(offset + 1).ok_or(MetadataError::InvalidUtf8)?;
                if second & 0xc0 != 0x80 {
                    return Err(MetadataError::InvalidUtf8);
                }
                utf16.push((u16::from(first & 0x1f) << 6) | u16::from(second & 0x3f));
                offset += 2;
            }
            0xe0..=0xef => {
                let second = *bytes.get(offset + 1).ok_or(MetadataError::InvalidUtf8)?;
                let third = *bytes.get(offset + 2).ok_or(MetadataError::InvalidUtf8)?;
                if second & 0xc0 != 0x80 || third & 0xc0 != 0x80 || (first == 0xe0 && second < 0xa0)
                {
                    return Err(MetadataError::InvalidUtf8);
                }
                utf16.push(
                    (u16::from(first & 0x0f) << 12)
                        | (u16::from(second & 0x3f) << 6)
                        | u16::from(third & 0x3f),
                );
                offset += 3;
            }
            0x00 | 0x80..=0xbf | 0xc1 | 0xf0..=0xff => {
                return Err(MetadataError::InvalidUtf8);
            }
        }
    }
    Ok(String::from_utf16(&utf16).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_hex(source: &str) -> Vec<u8> {
        let digits = source
            .bytes()
            .filter(|byte| !byte.is_ascii_whitespace())
            .collect::<Vec<_>>();
        assert_eq!(
            digits.len() % 2,
            0,
            "checked hex fixture must contain complete bytes"
        );
        digits
            .chunks_exact(2)
            .map(|pair| {
                let pair = std::str::from_utf8(pair).expect("fixture hex must be ASCII");
                u8::from_str_radix(pair, 16).expect("fixture byte must be valid hex")
            })
            .collect()
    }

    fn replace_ascii(bytes: &mut [u8], old: &[u8], new: &[u8]) {
        assert_eq!(old.len(), new.len(), "fixture mutation must preserve size");
        let mut replacements = 0;
        for offset in 0..=bytes.len() - old.len() {
            if &bytes[offset..offset + old.len()] == old {
                bytes[offset..offset + old.len()].copy_from_slice(new);
                replacements += 1;
            }
        }
        assert!(replacements > 0, "fixture mutation target must exist");
    }

    #[test]
    fn decodes_jvm_modified_utf8_null_and_supplementary_characters() {
        assert_eq!(
            decode_modified_utf8(&[b'A', 0xc0, 0x80, 0xed, 0xa0, 0xbd, 0xed, 0xb8, 0x80, b'Z']),
            Ok(Some("A\0😀Z".to_string()))
        );
    }

    #[test]
    fn rejects_malformed_modified_utf8_without_lossy_replacement() {
        for malformed in [
            &[0x00][..],
            &[0xc0, 0x81][..],
            &[0xe0, 0x80, 0x80][..],
            &[0xf0, 0x9f, 0x98, 0x80][..],
        ] {
            assert_eq!(
                decode_modified_utf8(malformed),
                Err(MetadataError::InvalidUtf8)
            );
        }
        assert_eq!(
            decode_modified_utf8(&[0xed, 0xa0, 0xbd]),
            Ok(None),
            "a legal unpaired Java UTF-16 surrogate must remain representable as an unused \
             constant-pool entry even though Rust strings cannot contain it"
        );
    }

    #[test]
    fn parses_checked_minimal_class_fixture() {
        let bytes = decode_hex(include_str!("../fixtures/minimal_class.hex"));
        let class = parse_class(&bytes, ClassLimits::default())
            .expect("checked minimal class fixture must parse");

        assert_eq!(class.major_version, 61);
        assert_eq!(class.name, "com/example/Demo");
        assert_eq!(class.super_name.as_deref(), Some("java/lang/Object"));
        assert_eq!(class.source_file.as_deref(), Some("Demo.java"));
        assert_eq!(class.methods.len(), 1);
        assert_eq!(class.methods[0].access_flags, 0x0001);
        assert_eq!(class.methods[0].name, "<init>");
        assert_eq!(class.methods[0].descriptor, "()V");
    }

    #[test]
    fn rejects_class_larger_than_configured_bound() {
        let bytes = decode_hex(include_str!("../fixtures/minimal_class.hex"));
        let mut limits = ClassLimits::default();
        limits.max_class_bytes = bytes.len() - 1;

        assert_eq!(
            parse_class(&bytes, limits),
            Err(MetadataError::LimitExceeded("class bytes"))
        );
    }

    #[test]
    fn rejects_truncated_and_corrupt_classfiles_explicitly() {
        let bytes = decode_hex(include_str!("../fixtures/rich_fixture.class.hex"));
        assert_eq!(
            parse_class(&bytes[..bytes.len() - 1], ClassLimits::default()),
            Err(MetadataError::Truncated)
        );

        let mut corrupt = bytes;
        corrupt[0] = 0;
        assert_eq!(
            parse_class(&corrupt, ClassLimits::default()),
            Err(MetadataError::InvalidMagic)
        );
    }

    #[test]
    fn enforces_constant_pool_and_attribute_bounds() {
        let bytes = decode_hex(include_str!("../fixtures/rich_fixture.class.hex"));
        let mut constant_pool_limits = ClassLimits::default();
        constant_pool_limits.max_constant_pool_entries = 1;
        assert_eq!(
            parse_class(&bytes, constant_pool_limits),
            Err(MetadataError::LimitExceeded("constant pool entries"))
        );

        let mut attribute_limits = ClassLimits::default();
        attribute_limits.max_attributes = 1;
        assert_eq!(
            parse_class(&bytes, attribute_limits),
            Err(MetadataError::LimitExceeded("attributes"))
        );
    }

    #[test]
    fn parses_generics_parameters_exceptions_annotations_and_lines() {
        let bytes = decode_hex(include_str!("../fixtures/rich_fixture.class.hex"));
        let class =
            parse_class(&bytes, ClassLimits::default()).expect("rich class fixture must parse");

        assert_eq!(class.name, "fixtures/RichFixture");
        assert_eq!(class.interfaces, vec!["java/lang/Runnable"]);
        assert_eq!(
            class.signature.as_deref(),
            Some("<T:Ljava/lang/Number;>Ljava/lang/Object;Ljava/lang/Runnable;")
        );
        assert_eq!(
            class.annotations,
            vec![AnnotationInfo {
                descriptor: "Lfixtures/Marker;".to_string(),
            }]
        );
        assert!(class
            .inner_classes
            .iter()
            .any(|inner| inner.inner_class == "fixtures/RichFixture$Inner"
                && inner.inner_name.as_deref() == Some("Inner")));

        let combine = class
            .methods
            .iter()
            .find(|method| method.name == "combine")
            .expect("rich fixture must contain combine");
        assert_eq!(combine.access_flags & 0x0080, 0x0080);
        assert_eq!(
            combine.signature.as_deref(),
            Some("(Ljava/lang/String;[I)Ljava/util/List<Ljava/lang/String;>;")
        );
        assert_eq!(combine.exceptions, vec!["java/io/IOException"]);
        assert_eq!(
            combine.parameters,
            vec![
                MethodParameter {
                    name: "prefix".to_string(),
                    access_flags: 0,
                },
                MethodParameter {
                    name: "values".to_string(),
                    access_flags: 0,
                },
            ]
        );
        assert_eq!(
            combine.annotations,
            vec![AnnotationInfo {
                descriptor: "Lfixtures/Marker;".to_string(),
            }]
        );
        assert_eq!(combine.first_line, Some(24));
    }

    #[test]
    fn classifies_annotation_enum_record_and_inner_class_fixtures() {
        let marker = parse_class(
            &decode_hex(include_str!("../fixtures/marker.class.hex")),
            ClassLimits::default(),
        )
        .expect("annotation fixture must parse");
        assert_eq!(marker.kind(), ClassKind::Annotation);
        assert_eq!(marker.interfaces, vec!["java/lang/annotation/Annotation"]);
        assert!(marker.methods.iter().any(|method| method.name == "value"));

        let shade = parse_class(
            &decode_hex(include_str!("../fixtures/shade.class.hex")),
            ClassLimits::default(),
        )
        .expect("enum fixture must parse");
        assert_eq!(shade.kind(), ClassKind::Enum);
        assert_eq!(shade.super_name.as_deref(), Some("java/lang/Enum"));
        assert!(shade
            .fields
            .iter()
            .any(|field| field.name == "RED" && field.access_flags & 0x4000 != 0));

        let point = parse_class(
            &decode_hex(include_str!("../fixtures/point.class.hex")),
            ClassLimits::default(),
        )
        .expect("record fixture must parse");
        assert_eq!(point.kind(), ClassKind::Record);
        assert_eq!(
            point.record_components,
            vec![
                RecordComponentInfo {
                    name: "x".to_string(),
                    descriptor: "I".to_string(),
                    signature: None,
                    annotations: Vec::new(),
                },
                RecordComponentInfo {
                    name: "y".to_string(),
                    descriptor: "I".to_string(),
                    signature: None,
                    annotations: Vec::new(),
                },
            ]
        );

        let inner = parse_class(
            &decode_hex(include_str!("../fixtures/inner.class.hex")),
            ClassLimits::default(),
        )
        .expect("inner class fixture must parse");
        assert_eq!(inner.kind(), ClassKind::Class);
        assert_eq!(inner.name, "fixtures/RichFixture$Inner");
        assert!(inner
            .fields
            .iter()
            .any(|field| field.name == "this$0" && field.access_flags & 0x1000 != 0));
    }

    #[test]
    fn uses_debug_parameter_names_then_deterministic_fallbacks() {
        let mut debug_only = decode_hex(include_str!("../fixtures/rich_fixture.class.hex"));
        replace_ascii(&mut debug_only, b"MethodParameters", b"IgnoredParamsXXX");
        let class = parse_class(&debug_only, ClassLimits::default())
            .expect("debug-only parameter fixture must parse");
        let combine = class
            .methods
            .iter()
            .find(|method| method.name == "combine")
            .expect("fixture must contain combine");
        assert_eq!(
            combine
                .parameters
                .iter()
                .map(|parameter| parameter.name.as_str())
                .collect::<Vec<_>>(),
            vec!["prefix", "values"]
        );

        replace_ascii(
            &mut debug_only,
            b"LocalVariableTable",
            b"IgnoredLocalAttrXX",
        );
        let class = parse_class(&debug_only, ClassLimits::default())
            .expect("metadata-free parameter fixture must parse");
        let combine = class
            .methods
            .iter()
            .find(|method| method.name == "combine")
            .expect("fixture must contain combine");
        assert_eq!(
            combine
                .parameters
                .iter()
                .map(|parameter| parameter.name.as_str())
                .collect::<Vec<_>>(),
            vec!["arg0", "arg1"]
        );
    }

    #[test]
    fn parses_module_identity_without_loading_the_module() {
        let module = parse_class(
            &decode_hex(include_str!("../fixtures/module-info.class.hex")),
            ClassLimits::default(),
        )
        .expect("checked module-info fixture must parse");
        assert_eq!(module.kind(), ClassKind::Module);
        assert_eq!(module.name, "module-info");
        assert_eq!(module.module_name.as_deref(), Some("fixtures.sample"));
        assert_eq!(module.source_file.as_deref(), Some("module-info.java"));
    }

    #[test]
    fn retains_overloads_and_member_flags_as_separate_declarations() {
        let class = parse_class(
            &decode_hex(include_str!("../fixtures/overloads.class.hex")),
            ClassLimits::default(),
        )
        .expect("checked overload fixture must parse");
        assert_eq!(class.kind(), ClassKind::Class);
        assert_eq!(class.access_flags & 0x0400, 0x0400);
        let overloads = class
            .methods
            .iter()
            .filter(|method| method.name == "value")
            .collect::<Vec<_>>();
        assert_eq!(overloads.len(), 2);
        assert_eq!(overloads[0].descriptor, "(I)Ljava/lang/String;");
        assert_eq!(
            overloads[1].descriptor,
            "(Ljava/lang/String;)Ljava/lang/String;"
        );

        let native = class
            .methods
            .iter()
            .find(|method| method.name == "nativeValue")
            .expect("fixture must contain nativeValue");
        assert_eq!(native.visibility(), Visibility::Protected);
        assert!(native.is_static());
        assert!(native.is_native());
        assert!(!native.is_abstract());
    }

    #[test]
    fn every_truncated_prefix_and_bounded_mutation_fails_without_panicking() {
        let bytes = decode_hex(include_str!("../fixtures/rich_fixture.class.hex"));
        for length in 0..bytes.len() {
            assert!(
                parse_class(&bytes[..length], ClassLimits::default()).is_err(),
                "truncated prefix of {length} bytes must not parse"
            );
        }

        let mut state = 0x8f4d_3b29_u32;
        for _ in 0..256 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let mut mutated = bytes.clone();
            let offset = usize::try_from(state).expect("u32 must fit usize") % mutated.len();
            mutated[offset] ^= ((state >> 24) as u8) | 1;
            let result = std::panic::catch_unwind(|| {
                let _ = parse_class(&mutated, ClassLimits::default());
            });
            assert!(result.is_ok(), "bounded malformed input must never panic");
        }
    }
}
