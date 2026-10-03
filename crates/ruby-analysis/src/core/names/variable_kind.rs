#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VariableTypeKind {
    Local,
    Instance,
    Class,
    Global,
    Constant,
}
