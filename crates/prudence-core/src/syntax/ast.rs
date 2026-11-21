//! Surface syntax tree of a Prudence file.

use super::lexer::Pos;

#[derive(Clone, Debug, PartialEq)]
pub struct Annotation {
    pub name: String,
    pub args: Vec<Arg>,
    pub pos: Pos,
}

impl Annotation {
    pub fn arg(&self, name: Option<&str>) -> Option<&Expr> {
        self.args.iter().find(|a| a.name.as_deref() == name).map(|a| &a.value)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Arg {
    pub name: Option<String>,
    pub value: Expr,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Implies,
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    In,
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Not,
    Neg,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sense {
    Maximize,
    Minimize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Num(String, Pos),
    Percent(String, Pos),
    Str(String, Pos),
    Ident(String, Pos),
    Cell(String, String, Pos),
    List(Vec<Expr>),
    Tuple(Vec<Expr>),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<Arg>),
    Index(Box<Expr>, Box<Expr>),
    Member(Box<Expr>, String),
    Is(Box<Expr>, String),
    If { cond: Box<Expr>, then: Box<Expr>, other: Box<Expr> },
    IfLet { pat: Pat, value: Box<Expr>, then: Box<Expr>, other: Box<Expr> },
    Match { scrutinee: Box<Expr>, arms: Vec<Arm> },
    Let { pat: Pat, value: Box<Expr>, body: Box<Expr> },
    /// `x in xs where cond | body` (inside an aggregate's parentheses).
    Comprehension { var: String, source: Box<Expr>, filter: Option<Box<Expr>>, body: Option<Box<Expr>> },
    /// `group Source by key where cond { field = agg ... }`; row fields are in scope.
    Group { source: Box<Expr>, key: Box<Expr>, filter: Option<Box<Expr>>, fields: Vec<(String, Expr)> },
    /// `optimize per x where cond { maximize e subject to c ... }`
    Optimize { per: String, filter: Option<Box<Expr>>, sense: Sense, objective: Box<Expr>, constraints: Vec<Expr> },
    /// `forall x: Type, body` or `forall x in source, body`
    Forall { var: String, domain: Domain, body: Box<Expr> },
    Annotated(Box<Expr>, Vec<Annotation>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Domain {
    Type(Type),
    In(Box<Expr>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Arm {
    pub pat: Pat,
    pub guard: Option<Expr>,
    pub body: Expr,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Pat {
    Wild,
    Bind(String),
    Ctor(String, Vec<Pat>),
    Tuple(Vec<Pat>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Type {
    /// `Name<generics>(params)`; params are `name: Type` fields or expressions.
    Named { name: String, generics: Vec<Type>, params: Vec<TypeParam>, pos: Pos },
    Union(Vec<Type>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum TypeParam {
    Field(String, Type),
    Expr(Expr),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub name: String,
    pub ty: Type,
    pub anns: Vec<Annotation>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Case {
    pub cond: Expr,
    pub lets: Vec<(Pat, Expr)>,
    pub outputs: Vec<(String, Expr)>,
}

// Declarations are few per module; size is irrelevant.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq)]
pub enum DeclKind {
    Param { ty: Type, default: Option<Expr> },
    TypeAlias(Type),
    Data { base: Option<Type>, fields: Vec<Field> },
    Event(Vec<Field>),
    View(Expr),
    Fn { params: Vec<Field>, ret: Type, body: Option<Expr> },
    Alloc { ty: Type, body: Expr },
    Partition {
        var: String,
        source: Type,
        targets: Vec<String>,
        split: Expr,
        split_var: String,
        split_by: Expr,
        /// Further nested splits: each divides every portion of the previous one;
        /// its amount may refer to the earlier split variables.
        more_splits: Vec<(String, Expr)>,
        cases: Vec<Case>,
    },
    Invariant(Expr),
    Theorem(Expr),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Decl {
    pub name: String,
    pub kind: DeclKind,
    pub anns: Vec<Annotation>,
    pub pos: Pos,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Module {
    pub name: String,
    pub params: Vec<Arg>,
    pub anns: Vec<Annotation>,
    pub decls: Vec<Decl>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CellDef {
    /// `input` or `input(default: 0.10)`
    Input(Option<Expr>),
    Expr(Expr),
}

#[derive(Clone, Debug, PartialEq)]
pub struct FormItem {
    pub id: String,
    pub label: String,
    /// `observer`: an "of which" / memo row, never summed into a total.
    pub observer: bool,
    pub cells: Vec<(String, CellDef)>,
    pub anns: Vec<Annotation>,
    pub pos: Pos,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FormCheck {
    /// `check warn ...`: reported, but does not make the return invalid.
    pub warn: bool,
    pub id: String,
    pub description: String,
    pub cond: Expr,
    pub anns: Vec<Annotation>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FormDecl {
    pub name: String,
    pub code: String,
    pub title: String,
    pub columns: Vec<String>,
    pub input: String,
    pub items: Vec<FormItem>,
    pub checks: Vec<FormCheck>,
    pub theorems: Vec<FormCheck>,
    pub anns: Vec<Annotation>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TopLevel {
    Module(Module),
    Form(FormDecl),
}

#[derive(Clone, Debug, PartialEq)]
pub struct File {
    pub items: Vec<TopLevel>,
}
