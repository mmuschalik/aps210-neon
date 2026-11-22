//! JSON loader for Prudence IR (`ast::File`) exported by the Lean 4 DSL.
//! Uses `serde_json::Value` directly so `prudence-core` needs no proc-macro deps.

use serde_json::Value;

use super::ast::*;

type R<T> = Result<T, String>;

fn obj(v: &Value) -> R<&serde_json::Map<String, Value>> {
    v.as_object().ok_or_else(|| format!("expected JSON object, got {v}"))
}

fn arr(v: &Value) -> R<&[Value]> {
    v.as_array().map(Vec::as_slice).ok_or_else(|| format!("expected JSON array, got {v}"))
}

fn str_val(v: &Value) -> R<String> {
    v.as_str().map(str::to_string).ok_or_else(|| format!("expected JSON string, got {v}"))
}

fn bool_val(v: &Value) -> R<bool> {
    v.as_bool().ok_or_else(|| format!("expected JSON bool, got {v}"))
}

fn usize_val(v: &Value) -> R<usize> {
    v.as_u64().map(|n| n as usize).ok_or_else(|| format!("expected JSON uint, got {v}"))
}

fn field<'a>(m: &'a serde_json::Map<String, Value>, k: &str) -> R<&'a Value> {
    m.get(k).ok_or_else(|| format!("missing field `{k}`"))
}

fn opt<T>(v: &Value, f: impl FnOnce(&Value) -> R<T>) -> R<Option<T>> {
    if v.is_null() { Ok(None) } else { f(v).map(Some) }
}

fn vec_of<T>(v: &Value, mut f: impl FnMut(&Value) -> R<T>) -> R<Vec<T>> {
    arr(v)?.iter().map(&mut f).collect()
}

fn variant(v: &Value) -> R<(&str, Option<&Value>)> {
    if let Some(s) = v.as_str() {
        return Ok((s, None));
    }
    let m = obj(v)?;
    if m.len() != 1 {
        return Err(format!("expected 1-key enum object, got {v}"));
    }
    let (k, val) = m.iter().next().unwrap();
    Ok((k.as_str(), Some(val)))
}

fn pos(v: &Value) -> R<Pos> {
    let m = obj(v)?;
    Ok(Pos { line: usize_val(field(m, "line")?)?, col: usize_val(field(m, "col")?)? })
}

fn bin_op(v: &Value) -> R<BinOp> {
    Ok(match str_val(v)?.as_str() {
        "Implies" => BinOp::Implies,
        "Or" => BinOp::Or,
        "And" => BinOp::And,
        "Eq" => BinOp::Eq,
        "Ne" => BinOp::Ne,
        "Lt" => BinOp::Lt,
        "Le" => BinOp::Le,
        "Gt" => BinOp::Gt,
        "Ge" => BinOp::Ge,
        "In" => BinOp::In,
        "Add" => BinOp::Add,
        "Sub" => BinOp::Sub,
        "Mul" => BinOp::Mul,
        "Div" => BinOp::Div,
        s => return Err(format!("unknown BinOp `{s}`")),
    })
}

fn un_op(v: &Value) -> R<UnOp> {
    Ok(match str_val(v)?.as_str() {
        "Not" => UnOp::Not,
        "Neg" => UnOp::Neg,
        s => return Err(format!("unknown UnOp `{s}`")),
    })
}

fn sense(v: &Value) -> R<Sense> {
    Ok(match str_val(v)?.as_str() {
        "Maximize" => Sense::Maximize,
        "Minimize" => Sense::Minimize,
        s => return Err(format!("unknown Sense `{s}`")),
    })
}

fn pat(v: &Value) -> R<Pat> {
    let (tag, payload) = variant(v)?;
    Ok(match tag {
        "Wild" => Pat::Wild,
        "Bind" => Pat::Bind(str_val(payload.ok_or("Bind missing payload")?)?),
        "Ctor" => {
            let xs = arr(payload.ok_or("Ctor missing payload")?)?;
            if xs.len() != 2 {
                return Err("Ctor expects 2 elements".into());
            }
            Pat::Ctor(str_val(&xs[0])?, vec_of(&xs[1], pat)?)
        }
        "Tuple" => Pat::Tuple(vec_of(payload.ok_or("Tuple missing payload")?, pat)?),
        s => return Err(format!("unknown Pat `{s}`")),
    })
}

fn arg(v: &Value) -> R<Arg> {
    let m = obj(v)?;
    Ok(Arg { name: opt(field(m, "name")?, str_val)?, value: expr(field(m, "value")?)? })
}

fn annotation(v: &Value) -> R<Annotation> {
    let m = obj(v)?;
    Ok(Annotation {
        name: str_val(field(m, "name")?)?,
        args: vec_of(field(m, "args")?, arg)?,
        pos: m.get("pos").map(pos).transpose()?.unwrap_or_default(),
    })
}

fn arm(v: &Value) -> R<Arm> {
    let m = obj(v)?;
    Ok(Arm { pat: pat(field(m, "pat")?)?, guard: opt(field(m, "guard")?, expr)?, body: expr(field(m, "body")?)? })
}

fn domain(v: &Value) -> R<Domain> {
    let (tag, payload) = variant(v)?;
    let p = payload.ok_or("Domain missing payload")?;
    Ok(match tag {
        "Type" => Domain::Type(ty(p)?),
        "In" => Domain::In(Box::new(expr(p)?)),
        s => return Err(format!("unknown Domain `{s}`")),
    })
}

fn ty_param(v: &Value) -> R<TypeParam> {
    let (tag, payload) = variant(v)?;
    let p = payload.ok_or("TypeParam missing payload")?;
    Ok(match tag {
        "Field" => {
            let xs = arr(p)?;
            if xs.len() != 2 {
                return Err("TypeParam::Field expects 2 elements".into());
            }
            TypeParam::Field(str_val(&xs[0])?, ty(&xs[1])?)
        }
        "Expr" => TypeParam::Expr(expr(p)?),
        s => return Err(format!("unknown TypeParam `{s}`")),
    })
}

fn ty(v: &Value) -> R<Type> {
    let (tag, payload) = variant(v)?;
    let p = payload.ok_or("Type missing payload")?;
    Ok(match tag {
        "Named" => {
            let m = obj(p)?;
            Type::Named {
                name: str_val(field(m, "name")?)?,
                generics: vec_of(field(m, "generics")?, ty)?,
                params: vec_of(field(m, "params")?, ty_param)?,
                pos: m.get("pos").map(pos).transpose()?.unwrap_or_default(),
            }
        }
        "Union" => Type::Union(vec_of(p, ty)?),
        s => return Err(format!("unknown Type `{s}`")),
    })
}

fn expr(v: &Value) -> R<Expr> {
    let (tag, payload) = variant(v)?;
    let p = payload.ok_or_else(|| format!("Expr::{tag} missing payload"))?;
    Ok(match tag {
        "Num" => {
            let xs = arr(p)?;
            Expr::Num(str_val(&xs[0])?, xs.get(1).map(pos).transpose()?.unwrap_or_default())
        }
        "Percent" => {
            let xs = arr(p)?;
            Expr::Percent(str_val(&xs[0])?, xs.get(1).map(pos).transpose()?.unwrap_or_default())
        }
        "Str" => {
            let xs = arr(p)?;
            Expr::Str(str_val(&xs[0])?, xs.get(1).map(pos).transpose()?.unwrap_or_default())
        }
        "Ident" => {
            let xs = arr(p)?;
            Expr::Ident(str_val(&xs[0])?, xs.get(1).map(pos).transpose()?.unwrap_or_default())
        }
        "Cell" => {
            let xs = arr(p)?;
            Expr::Cell(str_val(&xs[0])?, str_val(&xs[1])?, xs.get(2).map(pos).transpose()?.unwrap_or_default())
        }
        "List" => Expr::List(vec_of(p, expr)?),
        "Tuple" => Expr::Tuple(vec_of(p, expr)?),
        "Unary" => {
            let xs = arr(p)?;
            Expr::Unary(un_op(&xs[0])?, Box::new(expr(&xs[1])?))
        }
        "Binary" => {
            let xs = arr(p)?;
            Expr::Binary(bin_op(&xs[0])?, Box::new(expr(&xs[1])?), Box::new(expr(&xs[2])?))
        }
        "Call" => {
            let xs = arr(p)?;
            Expr::Call(Box::new(expr(&xs[0])?), vec_of(&xs[1], arg)?)
        }
        "Index" => {
            let xs = arr(p)?;
            Expr::Index(Box::new(expr(&xs[0])?), Box::new(expr(&xs[1])?))
        }
        "Member" => {
            let xs = arr(p)?;
            Expr::Member(Box::new(expr(&xs[0])?), str_val(&xs[1])?)
        }
        "Is" => {
            let xs = arr(p)?;
            Expr::Is(Box::new(expr(&xs[0])?), str_val(&xs[1])?)
        }
        "If" => {
            let m = obj(p)?;
            Expr::If {
                cond: Box::new(expr(field(m, "cond")?)?),
                then: Box::new(expr(field(m, "then")?)?),
                other: Box::new(expr(field(m, "other")?)?),
            }
        }
        "IfLet" => {
            let m = obj(p)?;
            Expr::IfLet {
                pat: pat(field(m, "pat")?)?,
                value: Box::new(expr(field(m, "value")?)?),
                then: Box::new(expr(field(m, "then")?)?),
                other: Box::new(expr(field(m, "other")?)?),
            }
        }
        "Match" => {
            let m = obj(p)?;
            Expr::Match { scrutinee: Box::new(expr(field(m, "scrutinee")?)?), arms: vec_of(field(m, "arms")?, arm)? }
        }
        "Let" => {
            let m = obj(p)?;
            Expr::Let {
                pat: pat(field(m, "pat")?)?,
                value: Box::new(expr(field(m, "value")?)?),
                body: Box::new(expr(field(m, "body")?)?),
            }
        }
        "Comprehension" => {
            let m = obj(p)?;
            Expr::Comprehension {
                var: str_val(field(m, "var")?)?,
                source: Box::new(expr(field(m, "source")?)?),
                filter: opt(field(m, "filter")?, |x| expr(x).map(Box::new))?,
                body: opt(field(m, "body")?, |x| expr(x).map(Box::new))?,
            }
        }
        "Group" => {
            let m = obj(p)?;
            Expr::Group {
                source: Box::new(expr(field(m, "source")?)?),
                key: Box::new(expr(field(m, "key")?)?),
                filter: opt(field(m, "filter")?, |x| expr(x).map(Box::new))?,
                fields: vec_of(field(m, "fields")?, |kv| {
                    let xs = arr(kv)?;
                    Ok((str_val(&xs[0])?, expr(&xs[1])?))
                })?,
            }
        }
        "Optimize" => {
            let m = obj(p)?;
            Expr::Optimize {
                per: str_val(field(m, "per")?)?,
                filter: opt(field(m, "filter")?, |x| expr(x).map(Box::new))?,
                sense: sense(field(m, "sense")?)?,
                objective: Box::new(expr(field(m, "objective")?)?),
                constraints: vec_of(field(m, "constraints")?, expr)?,
            }
        }
        "Forall" => {
            let m = obj(p)?;
            Expr::Forall {
                var: str_val(field(m, "var")?)?,
                domain: domain(field(m, "domain")?)?,
                body: Box::new(expr(field(m, "body")?)?),
            }
        }
        "Annotated" => {
            let xs = arr(p)?;
            Expr::Annotated(Box::new(expr(&xs[0])?), vec_of(&xs[1], annotation)?)
        }
        s => return Err(format!("unknown Expr `{s}`")),
    })
}

fn field_decl(v: &Value) -> R<Field> {
    let m = obj(v)?;
    Ok(Field {
        name: str_val(field(m, "name")?)?,
        ty: ty(field(m, "ty")?)?,
        anns: vec_of(field(m, "anns")?, annotation)?,
    })
}

fn case(v: &Value) -> R<Case> {
    let m = obj(v)?;
    Ok(Case {
        cond: expr(field(m, "cond")?)?,
        lets: vec_of(field(m, "lets")?, |kv| {
            let xs = arr(kv)?;
            Ok((pat(&xs[0])?, expr(&xs[1])?))
        })?,
        outputs: vec_of(field(m, "outputs")?, |kv| {
            let xs = arr(kv)?;
            Ok((str_val(&xs[0])?, expr(&xs[1])?))
        })?,
    })
}

fn decl_kind(v: &Value) -> R<DeclKind> {
    let (tag, payload) = variant(v)?;
    let p = payload.ok_or_else(|| format!("DeclKind::{tag} missing payload"))?;
    Ok(match tag {
        "Param" => {
            let m = obj(p)?;
            DeclKind::Param { ty: ty(field(m, "ty")?)?, default: opt(field(m, "default")?, expr)? }
        }
        "TypeAlias" => DeclKind::TypeAlias(ty(p)?),
        "Data" => {
            let m = obj(p)?;
            DeclKind::Data { base: opt(field(m, "base")?, ty)?, fields: vec_of(field(m, "fields")?, field_decl)? }
        }
        "Event" => DeclKind::Event(vec_of(p, field_decl)?),
        "View" => DeclKind::View(expr(p)?),
        "Fn" => {
            let m = obj(p)?;
            DeclKind::Fn {
                params: vec_of(field(m, "params")?, field_decl)?,
                ret: ty(field(m, "ret")?)?,
                body: opt(field(m, "body")?, expr)?,
            }
        }
        "Alloc" => {
            let m = obj(p)?;
            DeclKind::Alloc { ty: ty(field(m, "ty")?)?, body: expr(field(m, "body")?)? }
        }
        "Partition" => {
            let m = obj(p)?;
            DeclKind::Partition {
                var: str_val(field(m, "var")?)?,
                source: ty(field(m, "source")?)?,
                targets: vec_of(field(m, "targets")?, str_val)?,
                split: expr(field(m, "split")?)?,
                split_var: str_val(field(m, "split_var")?)?,
                split_by: expr(field(m, "split_by")?)?,
                more_splits: vec_of(field(m, "more_splits")?, |kv| {
                    let xs = arr(kv)?;
                    Ok((str_val(&xs[0])?, expr(&xs[1])?))
                })?,
                cases: vec_of(field(m, "cases")?, case)?,
            }
        }
        "Invariant" => DeclKind::Invariant(expr(p)?),
        "Theorem" => DeclKind::Theorem(expr(p)?),
        s => return Err(format!("unknown DeclKind `{s}`")),
    })
}

fn decl(v: &Value) -> R<Decl> {
    let m = obj(v)?;
    Ok(Decl {
        name: str_val(field(m, "name")?)?,
        kind: decl_kind(field(m, "kind")?)?,
        anns: vec_of(field(m, "anns")?, annotation)?,
        pos: m.get("pos").map(pos).transpose()?.unwrap_or_default(),
    })
}

fn module(v: &Value) -> R<Module> {
    let m = obj(v)?;
    Ok(Module {
        name: str_val(field(m, "name")?)?,
        params: vec_of(field(m, "params")?, arg)?,
        anns: vec_of(field(m, "anns")?, annotation)?,
        decls: vec_of(field(m, "decls")?, decl)?,
    })
}

fn cell_def(v: &Value) -> R<CellDef> {
    let (tag, payload) = variant(v)?;
    let p = payload.ok_or_else(|| format!("CellDef::{tag} missing payload"))?;
    Ok(match tag {
        "Input" => CellDef::Input(opt(p, expr)?),
        "Expr" => CellDef::Expr(expr(p)?),
        s => return Err(format!("unknown CellDef `{s}`")),
    })
}

fn form_item(v: &Value) -> R<FormItem> {
    let m = obj(v)?;
    Ok(FormItem {
        id: str_val(field(m, "id")?)?,
        label: str_val(field(m, "label")?)?,
        observer: bool_val(field(m, "observer")?)?,
        cells: vec_of(field(m, "cells")?, |kv| {
            let xs = arr(kv)?;
            Ok((str_val(&xs[0])?, cell_def(&xs[1])?))
        })?,
        anns: vec_of(field(m, "anns")?, annotation)?,
        pos: m.get("pos").map(pos).transpose()?.unwrap_or_default(),
    })
}

fn form_check(v: &Value) -> R<FormCheck> {
    let m = obj(v)?;
    Ok(FormCheck {
        warn: bool_val(field(m, "warn")?)?,
        id: str_val(field(m, "id")?)?,
        description: str_val(field(m, "description")?)?,
        cond: expr(field(m, "cond")?)?,
        anns: vec_of(field(m, "anns")?, annotation)?,
    })
}

fn form_decl(v: &Value) -> R<FormDecl> {
    let m = obj(v)?;
    Ok(FormDecl {
        name: str_val(field(m, "name")?)?,
        code: str_val(field(m, "code")?)?,
        title: str_val(field(m, "title")?)?,
        columns: vec_of(field(m, "columns")?, str_val)?,
        input: str_val(field(m, "input")?)?,
        items: vec_of(field(m, "items")?, form_item)?,
        checks: vec_of(field(m, "checks")?, form_check)?,
        theorems: vec_of(field(m, "theorems")?, form_check)?,
        anns: vec_of(field(m, "anns")?, annotation)?,
    })
}

fn top_level(v: &Value) -> R<TopLevel> {
    let (tag, payload) = variant(v)?;
    let p = payload.ok_or_else(|| format!("TopLevel::{tag} missing payload"))?;
    Ok(match tag {
        "Module" => TopLevel::Module(module(p)?),
        "Form" => TopLevel::Form(form_decl(p)?),
        s => return Err(format!("unknown TopLevel `{s}`")),
    })
}

/// Minimal JSON parser into `serde_json::Value` without serde_json's 128-level
/// recursion cap (needed for deeply nested left-associative sum expressions in ARF 210.6).
struct JsonParser<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> JsonParser<'a> {
    fn ws(&mut self) {
        while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }
    fn peek(&mut self) -> Option<u8> {
        self.ws();
        self.b.get(self.i).copied()
    }
    fn parse_value(&mut self) -> R<Value> {
        match self.peek().ok_or("unexpected EOF")? {
            b'{' => self.parse_obj(),
            b'[' => self.parse_arr(),
            b'"' => Ok(Value::String(self.parse_str()?)),
            b't' => self.lit(b"true", Value::Bool(true)),
            b'f' => self.lit(b"false", Value::Bool(false)),
            b'n' => self.lit(b"null", Value::Null),
            b'-' | b'0'..=b'9' => self.parse_num(),
            c => Err(format!("unexpected byte `{}` at {}", c as char, self.i)),
        }
    }
    fn lit(&mut self, s: &[u8], v: Value) -> R<Value> {
        if self.b[self.i..].starts_with(s) {
            self.i += s.len();
            Ok(v)
        } else {
            Err(format!("expected literal at {}", self.i))
        }
    }
    fn parse_str(&mut self) -> R<String> {
        self.ws();
        let start = self.i;
        if self.b.get(self.i) != Some(&b'"') {
            return Err(format!("expected '\"' at {}", self.i));
        }
        self.i += 1;
        while self.i < self.b.len() {
            match self.b[self.i] {
                b'\\' => self.i += 2,
                b'"' => {
                    self.i += 1;
                    let slice = std::str::from_utf8(&self.b[start..self.i]).map_err(|e| e.to_string())?;
                    return serde_json::from_str(slice).map_err(|e| e.to_string());
                }
                _ => self.i += 1,
            }
        }
        Err("unterminated string".into())
    }
    fn parse_num(&mut self) -> R<Value> {
        let start = self.i;
        while self.i < self.b.len() && matches!(self.b[self.i], b'-' | b'+' | b'0'..=b'9' | b'.' | b'e' | b'E') {
            self.i += 1;
        }
        let slice = std::str::from_utf8(&self.b[start..self.i]).map_err(|e| e.to_string())?;
        serde_json::from_str(slice).map_err(|e| e.to_string())
    }
    fn parse_arr(&mut self) -> R<Value> {
        self.i += 1; // '['
        let mut xs = Vec::new();
        if self.peek() == Some(b']') {
            self.i += 1;
            return Ok(Value::Array(xs));
        }
        loop {
            xs.push(self.parse_value()?);
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b']') => {
                    self.i += 1;
                    return Ok(Value::Array(xs));
                }
                _ => return Err(format!("expected ',' or ']' at {}", self.i)),
            }
        }
    }
    fn parse_obj(&mut self) -> R<Value> {
        self.i += 1; // '{'
        let mut m = serde_json::Map::new();
        if self.peek() == Some(b'}') {
            self.i += 1;
            return Ok(Value::Object(m));
        }
        loop {
            let k = self.parse_str()?;
            if self.peek() != Some(b':') {
                return Err(format!("expected ':' at {}", self.i));
            }
            self.i += 1;
            let v = self.parse_value()?;
            m.insert(k, v);
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b'}') => {
                    self.i += 1;
                    return Ok(Value::Object(m));
                }
                _ => return Err(format!("expected ',' or '}}' at {}", self.i)),
            }
        }
    }
}

/// Deserialize a JSON-encoded Prudence IR `File` (emitted by `Prudence.JSON`).
pub fn decode_file(json: &str) -> R<File> {
    let mut p = JsonParser { b: json.as_bytes(), i: 0 };
    let root = p.parse_value().map_err(|e| format!("invalid JSON: {e}"))?;
    let m = obj(&root)?;
    Ok(File { items: vec_of(field(m, "items")?, top_level)? })
}
