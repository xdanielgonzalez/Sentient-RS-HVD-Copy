//! The IDL abstract syntax tree — a faithful, un-resolved parse of the annotated OMG-IDL subset.
//! `resolve.rs` turns this into the descriptor model.

/// A whole parsed IDL translation unit (top-level items — normally the single `module tms { … }`).
#[derive(Debug, Clone)]
pub struct Idl {
    pub items: Vec<Item>,
}

#[derive(Debug, Clone)]
pub enum Item {
    Module(Module),
    Struct(StructDef),
    Enum(EnumDef),
    Typedef(TypedefDef),
    Const(ConstDef),
}

#[derive(Debug, Clone)]
pub struct Module {
    pub name: String,
    pub items: Vec<Item>,
    pub line: u32,
}

#[derive(Debug, Clone)]
pub struct StructDef {
    pub name: String,
    pub anns: Vec<Ann>,
    pub members: Vec<Member>,
    pub line: u32,
}

#[derive(Debug, Clone)]
pub struct EnumDef {
    pub name: String,
    pub anns: Vec<Ann>,
    /// Enumerator spellings in declaration order (value-less; ordinals are the index).
    pub variants: Vec<String>,
    pub line: u32,
}

#[derive(Debug, Clone)]
pub struct TypedefDef {
    pub name: String,
    pub ty: TypeRef,
    pub line: u32,
}

#[derive(Debug, Clone)]
pub struct ConstDef {
    pub name: String,
    pub value: Expr,
    pub line: u32,
}

#[derive(Debug, Clone)]
pub struct Member {
    pub name: String,
    pub ty: TypeRef,
    pub anns: Vec<Ann>,
    pub line: u32,
}

/// A syntactic type reference, un-resolved (typedefs not yet chased, bounds not yet evaluated).
#[derive(Debug, Clone)]
pub enum TypeRef {
    /// A primitive keyword (`float`, `uint32`, …) OR a named/qualified type (`tms::Identity`).
    Named(String),
    /// `sequence<ELEM, BOUND>`.
    Sequence(Box<TypeRef>, Bound),
    /// `string<BOUND>` / `string<MIN,MAX>` (MAX kept).
    BoundedString(Bound),
    /// `char NAME[BOUND]` — a fixed-length character array (typedef position only).
    CharArray(Bound),
}

/// A sequence/string/array length: either a literal or a `const` identifier (possibly qualified).
#[derive(Debug, Clone)]
pub enum Bound {
    Lit(u64),
    Ident(String),
}

#[derive(Debug, Clone)]
pub enum Ann {
    Topic,
    Nested,
    Key,
    Optional,
    Extensibility(String),
    Unit(String),
    Range(RangeVal, RangeVal),
    Min(RangeVal),
    Max(RangeVal),
    /// Any annotation we recognize syntactically but don't model.
    Other(String),
}

/// A numeric annotation argument: a literal or a (possibly negated) `#define`/`const` identifier
/// (e.g. `-RADIAN_MAX`).
#[derive(Debug, Clone)]
pub enum RangeVal {
    Num(f64),
    Ident { name: String, negated: bool },
}

#[derive(Debug, Clone)]
pub enum Expr {
    Int(i64),
    Float(f64),
    Str(String),
    Ident(String),
}
