//! Recursive-descent parser for Prudence (design.txt §4.A syntax).
//!
//! Layout rule: a call `(` or index `[` must be on the same line as what it
//! applies to; everything else may continue across lines. That lets match
//! arms, constraints and case outputs be separated by newlines alone.

use super::ast::*;
use super::lexer::{Pos, Tok, Token, lex};

pub fn parse_file(src: &str) -> Result<File, String> {
    let toks = lex(src)?;
    let mut p = Parser { t: toks, i: 0 };
    let mut items = Vec::new();
    while !p.at_eof() {
        let anns = p.annotations()?;
        if p.eat_kw("module") {
            items.push(TopLevel::Module(p.module(anns)?));
        } else if p.eat_kw("form") {
            items.push(TopLevel::Form(p.form(anns)?));
        } else {
            return Err(p.err("expected `module` or `form`"));
        }
    }
    Ok(File { items })
}

/// Parse a single expression (used by tests and tools).
pub fn parse_expr(src: &str) -> Result<Expr, String> {
    let mut p = Parser { t: lex(src)?, i: 0 };
    let e = p.expr()?;
    if !p.at_eof() {
        return Err(p.err("unexpected trailing input"));
    }
    Ok(e)
}

const RESERVED: [&str; 12] = ["if", "then", "else", "where", "in", "is", "by", "case", "let", "match", "subject", "forall"];

struct Parser {
    t: Vec<Token>,
    i: usize,
}

impl Parser {
    // ------------------------------------------------------------------ token helpers
    fn tok(&self) -> &Token {
        &self.t[self.i]
    }
    fn pos(&self) -> Pos {
        self.tok().pos
    }
    fn at_eof(&self) -> bool {
        self.tok().tok == Tok::Eof
    }
    fn err(&self, m: &str) -> String {
        format!("{}: {m}, found {}", self.pos(), describe(&self.tok().tok))
    }
    fn is_sym(&self, s: &str) -> bool {
        matches!(&self.tok().tok, Tok::Sym(x) if *x == s)
    }
    fn is_kw(&self, k: &str) -> bool {
        matches!(&self.tok().tok, Tok::Ident(x) if x == k)
    }
    fn eat_sym(&mut self, s: &str) -> bool {
        if self.is_sym(s) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn eat_kw(&mut self, k: &str) -> bool {
        if self.is_kw(k) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn expect_sym(&mut self, s: &str) -> Result<(), String> {
        if self.eat_sym(s) { Ok(()) } else { Err(self.err(&format!("expected `{s}`"))) }
    }
    fn expect_kw(&mut self, k: &str) -> Result<(), String> {
        if self.eat_kw(k) { Ok(()) } else { Err(self.err(&format!("expected `{k}`"))) }
    }
    fn ident(&mut self) -> Result<String, String> {
        match &self.tok().tok {
            Tok::Ident(s) => {
                let s = s.clone();
                self.i += 1;
                Ok(s)
            }
            _ => Err(self.err("expected a name")),
        }
    }
    fn string(&mut self) -> Result<String, String> {
        match &self.tok().tok {
            Tok::Str(s) => {
                let s = s.clone();
                self.i += 1;
                Ok(s)
            }
            _ => Err(self.err("expected a string")),
        }
    }
    fn same_line(&self) -> bool {
        !self.tok().nl_before
    }
    fn next_is_sym(&self, s: &str) -> bool {
        matches!(&self.t.get(self.i + 1).map(|t| &t.tok), Some(Tok::Sym(x)) if *x == s)
    }

    // ------------------------------------------------------------------ annotations
    fn annotations(&mut self) -> Result<Vec<Annotation>, String> {
        let mut out = Vec::new();
        while self.is_sym("@") {
            let pos = self.pos();
            self.i += 1;
            let name = self.ident()?;
            let args = if self.is_sym("(") && self.same_line() { self.args()? } else { Vec::new() };
            out.push(Annotation { name, args, pos });
        }
        Ok(out)
    }

    fn args(&mut self) -> Result<Vec<Arg>, String> {
        self.expect_sym("(")?;
        let mut args = Vec::new();
        while !self.is_sym(")") {
            let name = if matches!(self.tok().tok, Tok::Ident(_)) && self.next_is_sym(":") {
                let n = self.ident()?;
                self.expect_sym(":")?;
                Some(n)
            } else {
                None
            };
            let mut value = self.expr()?;
            // Comprehension: `x in xs where cond | body`
            if self.is_kw("where") || self.is_sym("|") {
                let Expr::Binary(BinOp::In, var, source) = value else {
                    return Err(self.err("a comprehension starts with `name in source`"));
                };
                let Expr::Ident(var, _) = *var else {
                    return Err(self.err("comprehension variable must be a name"));
                };
                let filter = if self.eat_kw("where") { Some(Box::new(self.expr()?)) } else { None };
                let body = if self.eat_sym("|") { Some(Box::new(self.expr()?)) } else { None };
                value = Expr::Comprehension { var, source, filter, body };
            }
            args.push(Arg { name, value });
            if !self.eat_sym(",") {
                break;
            }
        }
        self.expect_sym(")")?;
        Ok(args)
    }

    /// Move annotations wrapped around a trailing expression onto the enclosing declaration.
    /// A trailing `@cite(..)` binds to the last operand while parsing
    /// (`a >= 0 @cite(..)` annotates `0`); at declaration level it belongs to
    /// the whole declaration, so lift it out of the rightmost operand.
    fn unwrap_anns(e: Expr, anns: &mut Vec<Annotation>) -> Expr {
        match e {
            Expr::Annotated(inner, a) => {
                let inner = Self::unwrap_anns(*inner, anns);
                anns.extend(a);
                inner
            }
            Expr::Binary(op, l, r) => Expr::Binary(op, l, Box::new(Self::unwrap_anns(*r, anns))),
            other => other,
        }
    }

    // ------------------------------------------------------------------ module
    fn module(&mut self, anns: Vec<Annotation>) -> Result<Module, String> {
        let name = self.ident()?;
        let params = if self.is_sym("(") { self.args()? } else { Vec::new() };
        self.expect_sym("{")?;
        let mut decls = Vec::new();
        while !self.eat_sym("}") {
            if self.at_eof() {
                return Err(self.err("unclosed module"));
            }
            decls.push(self.decl()?);
        }
        Ok(Module { name, params, anns, decls })
    }

    fn decl(&mut self) -> Result<Decl, String> {
        let mut anns = self.annotations()?;
        let pos = self.pos();
        let kw = self.ident()?;
        let (name, kind) = match kw.as_str() {
            "param" => {
                let name = self.ident()?;
                self.expect_sym(":")?;
                let ty = self.ty()?;
                let default = if self.eat_sym("=") { Some(Self::unwrap_anns(self.expr()?, &mut anns)) } else { None };
                (name, DeclKind::Param { ty, default })
            }
            "type" => {
                let name = self.ident()?;
                self.expect_sym("=")?;
                (name, DeclKind::TypeAlias(self.ty()?))
            }
            "data" => {
                let name = self.ident()?;
                let base = if self.eat_sym(":") { Some(self.ty()?) } else { None };
                self.expect_sym("{")?;
                let mut fields = Vec::new();
                while !self.eat_sym("}") {
                    fields.push(self.field()?);
                    self.eat_sym(",");
                }
                (name, DeclKind::Data { base, fields })
            }
            "event" => {
                let name = self.ident()?;
                (name, DeclKind::Event(self.field_list()?))
            }
            "view" => {
                let name = self.ident()?;
                self.expect_sym("=")?;
                (name, DeclKind::View(Self::unwrap_anns(self.expr()?, &mut anns)))
            }
            "fn" | "extern" => {
                if kw == "extern" {
                    self.expect_kw("fn")?;
                }
                let name = self.ident()?;
                let params = self.field_list()?;
                self.expect_sym("->")?;
                let ret = self.ty()?;
                let body = if kw == "fn" {
                    self.expect_sym("=")?;
                    Some(Self::unwrap_anns(self.expr()?, &mut anns))
                } else {
                    None
                };
                (name, DeclKind::Fn { params, ret, body })
            }
            "alloc" => {
                let name = self.ident()?;
                self.expect_sym(":")?;
                let ty = self.ty()?;
                self.expect_sym("=")?;
                (name, DeclKind::Alloc { ty, body: Self::unwrap_anns(self.expr()?, &mut anns) })
            }
            "partition" => {
                let var = self.ident()?;
                self.expect_sym(":")?;
                let source = self.ty()?;
                self.expect_kw("into")?;
                let mut targets = vec![self.ident()?];
                while self.eat_sym(",") {
                    targets.push(self.ident()?);
                }
                self.expect_kw("splitting")?;
                let split = self.expr()?;
                self.expect_kw("by")?;
                let split_var = self.ident()?;
                self.expect_sym(":")?;
                let split_by = self.expr()?;
                let mut more_splits = Vec::new();
                while self.eat_sym(",") {
                    let n = self.ident()?;
                    self.expect_sym(":")?;
                    more_splits.push((n, self.expr()?));
                }
                self.expect_sym("{")?;
                let mut cases = Vec::new();
                while !self.eat_sym("}") {
                    cases.push(self.case()?);
                }
                (format!("partition {var}"), DeclKind::Partition { var, source, targets, split, split_var, split_by, more_splits, cases })
            }
            "invariant" | "theorem" => {
                let name = self.ident()?;
                self.expect_sym(":")?;
                let body = Self::unwrap_anns(self.expr()?, &mut anns);
                (name, if kw == "invariant" { DeclKind::Invariant(body) } else { DeclKind::Theorem(body) })
            }
            other => return Err(format!("{pos}: unknown declaration `{other}`")),
        };
        anns.extend(self.annotations()?);
        Ok(Decl { name, kind, anns, pos })
    }

    fn field(&mut self) -> Result<Field, String> {
        let name = self.ident()?;
        self.expect_sym(":")?;
        let ty = self.ty()?;
        let anns = self.annotations()?;
        Ok(Field { name, ty, anns })
    }

    fn field_list(&mut self) -> Result<Vec<Field>, String> {
        self.expect_sym("(")?;
        let mut out = Vec::new();
        while !self.is_sym(")") {
            out.push(self.field()?);
            if !self.eat_sym(",") {
                break;
            }
        }
        self.expect_sym(")")?;
        Ok(out)
    }

    fn case(&mut self) -> Result<Case, String> {
        self.expect_kw("case")?;
        let cond = self.expr()?;
        self.expect_sym("=>")?;
        let (mut lets, mut outputs) = (Vec::new(), Vec::new());
        while !self.is_kw("case") && !self.is_sym("}") {
            if self.eat_kw("let") {
                let pat = self.pat()?;
                self.expect_sym("=")?;
                lets.push((pat, self.expr()?));
            } else {
                let name = self.ident()?;
                self.expect_sym(":")?;
                outputs.push((name, self.expr()?));
            }
        }
        Ok(Case { cond, lets, outputs })
    }

    // ------------------------------------------------------------------ types
    fn ty(&mut self) -> Result<Type, String> {
        let mut alts = vec![self.ty_atom()?];
        while self.eat_sym("|") {
            alts.push(self.ty_atom()?);
        }
        Ok(if alts.len() == 1 { alts.pop().unwrap() } else { Type::Union(alts) })
    }

    fn ty_atom(&mut self) -> Result<Type, String> {
        let pos = self.pos();
        let name = self.ident()?;
        let mut generics = Vec::new();
        if self.eat_sym("<") {
            loop {
                generics.push(self.ty()?);
                if !self.eat_sym(",") {
                    break;
                }
            }
            self.expect_sym(">")?;
        }
        let mut params = Vec::new();
        if self.is_sym("(") && self.same_line() {
            self.i += 1;
            while !self.is_sym(")") {
                if matches!(self.tok().tok, Tok::Ident(_)) && self.next_is_sym(":") {
                    let n = self.ident()?;
                    self.expect_sym(":")?;
                    params.push(TypeParam::Field(n, self.ty()?));
                } else {
                    params.push(TypeParam::Expr(self.expr()?));
                }
                if !self.eat_sym(",") {
                    break;
                }
            }
            self.expect_sym(")")?;
        }
        Ok(Type::Named { name, generics, params, pos })
    }

    // ------------------------------------------------------------------ patterns
    fn pat(&mut self) -> Result<Pat, String> {
        if self.eat_sym("_") {
            return Ok(Pat::Wild);
        }
        if self.eat_sym("(") {
            let mut ps = Vec::new();
            while !self.is_sym(")") {
                ps.push(self.pat()?);
                if !self.eat_sym(",") {
                    break;
                }
            }
            self.expect_sym(")")?;
            return Ok(Pat::Tuple(ps));
        }
        let name = self.ident()?;
        if self.is_sym("(") && self.same_line() {
            self.i += 1;
            let mut ps = Vec::new();
            while !self.is_sym(")") {
                ps.push(self.pat()?);
                if !self.eat_sym(",") {
                    break;
                }
            }
            self.expect_sym(")")?;
            return Ok(Pat::Ctor(name, ps));
        }
        Ok(if name.starts_with(|c: char| c.is_uppercase()) { Pat::Ctor(name, Vec::new()) } else { Pat::Bind(name) })
    }

    // ------------------------------------------------------------------ expressions
    fn expr(&mut self) -> Result<Expr, String> {
        let lhs = self.or()?;
        if self.eat_sym("==>") {
            let rhs = self.expr()?;
            return Ok(Expr::Binary(BinOp::Implies, Box::new(lhs), Box::new(rhs)));
        }
        Ok(lhs)
    }

    fn or(&mut self) -> Result<Expr, String> {
        let mut e = self.and()?;
        while self.eat_sym("||") {
            e = Expr::Binary(BinOp::Or, Box::new(e), Box::new(self.and()?));
        }
        Ok(e)
    }

    fn and(&mut self) -> Result<Expr, String> {
        let mut e = self.cmp()?;
        while self.eat_sym("&&") {
            e = Expr::Binary(BinOp::And, Box::new(e), Box::new(self.cmp()?));
        }
        Ok(e)
    }

    /// Comparisons chain: `a <= b <= c` means `a <= b && b <= c`.
    fn cmp(&mut self) -> Result<Expr, String> {
        let first = self.add()?;
        let mut terms: Vec<(BinOp, Expr)> = Vec::new();
        let mut lhs = first.clone();
        loop {
            let op = match &self.tok().tok {
                Tok::Sym("==") => BinOp::Eq,
                Tok::Sym("!=") => BinOp::Ne,
                Tok::Sym("<") => BinOp::Lt,
                Tok::Sym("<=") => BinOp::Le,
                Tok::Sym(">") => BinOp::Gt,
                Tok::Sym(">=") => BinOp::Ge,
                Tok::Ident(k) if k == "in" => BinOp::In,
                Tok::Ident(k) if k == "is" => {
                    self.i += 1;
                    let ty = self.ident()?;
                    lhs = Expr::Is(Box::new(lhs), ty);
                    if terms.is_empty() {
                        return Ok(lhs);
                    }
                    continue;
                }
                _ => break,
            };
            self.i += 1;
            let rhs = self.add()?;
            terms.push((op, Expr::Binary(op, Box::new(lhs), Box::new(rhs.clone()))));
            lhs = rhs;
        }
        let mut it = terms.into_iter().map(|(_, e)| e);
        match it.next() {
            None => Ok(lhs),
            Some(first) => Ok(it.fold(first, |acc, e| Expr::Binary(BinOp::And, Box::new(acc), Box::new(e)))),
        }
    }

    fn add(&mut self) -> Result<Expr, String> {
        let mut e = self.mul()?;
        loop {
            let op = if self.is_sym("+") {
                BinOp::Add
            } else if self.is_sym("-") {
                BinOp::Sub
            } else {
                break;
            };
            self.i += 1;
            e = Expr::Binary(op, Box::new(e), Box::new(self.mul()?));
        }
        Ok(e)
    }

    fn mul(&mut self) -> Result<Expr, String> {
        let mut e = self.unary()?;
        loop {
            let op = if self.is_sym("*") {
                BinOp::Mul
            } else if self.is_sym("/") {
                BinOp::Div
            } else {
                break;
            };
            self.i += 1;
            e = Expr::Binary(op, Box::new(e), Box::new(self.unary()?));
        }
        Ok(e)
    }

    fn unary(&mut self) -> Result<Expr, String> {
        if self.eat_sym("!") {
            return Ok(Expr::Unary(UnOp::Not, Box::new(self.unary()?)));
        }
        if self.eat_sym("-") {
            return Ok(Expr::Unary(UnOp::Neg, Box::new(self.unary()?)));
        }
        self.postfix()
    }

    fn postfix(&mut self) -> Result<Expr, String> {
        let mut e = self.primary()?;
        loop {
            if self.is_sym("(") && self.same_line() {
                e = Expr::Call(Box::new(e), self.args()?);
            } else if self.is_sym("[") && self.same_line() {
                self.i += 1;
                let idx = self.expr()?;
                self.expect_sym("]")?;
                e = Expr::Index(Box::new(e), Box::new(idx));
            } else if self.eat_sym(".") {
                e = Expr::Member(Box::new(e), self.ident()?);
            } else if self.is_sym("@") {
                let anns = self.annotations()?;
                e = Expr::Annotated(Box::new(e), anns);
            } else {
                return Ok(e);
            }
        }
    }

    fn primary(&mut self) -> Result<Expr, String> {
        let pos = self.pos();
        match self.tok().tok.clone() {
            Tok::Num(n) => {
                self.i += 1;
                Ok(Expr::Num(n, pos))
            }
            Tok::Percent(n) => {
                self.i += 1;
                Ok(Expr::Percent(n, pos))
            }
            Tok::Str(s) => {
                self.i += 1;
                Ok(Expr::Str(s, pos))
            }
            Tok::Cell(item, col) => {
                self.i += 1;
                Ok(Expr::Cell(item, col, pos))
            }
            Tok::Sym("(") => {
                self.i += 1;
                let mut xs = vec![self.expr()?];
                while self.eat_sym(",") {
                    xs.push(self.expr()?);
                }
                self.expect_sym(")")?;
                Ok(if xs.len() == 1 { xs.pop().unwrap() } else { Expr::Tuple(xs) })
            }
            Tok::Sym("[") => {
                self.i += 1;
                let mut xs = Vec::new();
                while !self.is_sym("]") {
                    xs.push(self.expr()?);
                    if !self.eat_sym(",") {
                        break;
                    }
                }
                self.expect_sym("]")?;
                Ok(Expr::List(xs))
            }
            Tok::Ident(k) => match k.as_str() {
                "if" => self.if_expr(),
                "match" => {
                    self.i += 1;
                    let scrutinee = Box::new(self.expr()?);
                    self.expect_sym("{")?;
                    let mut arms = Vec::new();
                    while !self.eat_sym("}") {
                        let pat = self.pat()?;
                        let guard = if self.eat_kw("if") { Some(self.expr()?) } else { None };
                        self.expect_sym("=>")?;
                        let body = self.expr()?;
                        self.eat_sym(",");
                        arms.push(Arm { pat, guard, body });
                    }
                    Ok(Expr::Match { scrutinee, arms })
                }
                "let" => {
                    self.i += 1;
                    let pat = self.pat()?;
                    self.expect_sym("=")?;
                    let value = Box::new(self.expr()?);
                    let body = Box::new(self.expr()?);
                    Ok(Expr::Let { pat, value, body })
                }
                "group" => {
                    self.i += 1;
                    let source = Box::new(self.expr()?);
                    self.expect_kw("by")?;
                    let key = Box::new(self.expr()?);
                    let filter = if self.eat_kw("where") { Some(Box::new(self.expr()?)) } else { None };
                    self.expect_sym("{")?;
                    let mut fields = Vec::new();
                    while !self.eat_sym("}") {
                        let n = self.ident()?;
                        self.expect_sym("=")?;
                        fields.push((n, self.expr()?));
                        self.eat_sym(",");
                    }
                    Ok(Expr::Group { source, key, filter, fields })
                }
                "optimize" => {
                    self.i += 1;
                    self.expect_kw("per")?;
                    let per = self.ident()?;
                    let filter = if self.eat_kw("where") { Some(Box::new(self.expr()?)) } else { None };
                    self.expect_sym("{")?;
                    let sense = if self.eat_kw("maximize") {
                        Sense::Maximize
                    } else if self.eat_kw("minimize") {
                        Sense::Minimize
                    } else {
                        return Err(self.err("expected `maximize` or `minimize`"));
                    };
                    let objective = Box::new(self.expr()?);
                    self.expect_kw("subject")?;
                    self.expect_kw("to")?;
                    let mut constraints = Vec::new();
                    while !self.eat_sym("}") {
                        constraints.push(self.expr()?);
                        self.eat_sym(",");
                    }
                    Ok(Expr::Optimize { per, filter, sense, objective, constraints })
                }
                "forall" => {
                    self.i += 1;
                    let var = self.ident()?;
                    let domain = if self.eat_sym(":") {
                        Domain::Type(self.ty()?)
                    } else {
                        self.expect_kw("in")?;
                        Domain::In(Box::new(self.add()?))
                    };
                    self.expect_sym(",")?;
                    Ok(Expr::Forall { var, domain, body: Box::new(self.expr()?) })
                }
                r if RESERVED.contains(&r) => Err(self.err("expected an expression")),
                _ => {
                    self.i += 1;
                    Ok(Expr::Ident(k, pos))
                }
            },
            _ => Err(self.err("expected an expression")),
        }
    }

    fn if_expr(&mut self) -> Result<Expr, String> {
        self.expect_kw("if")?;
        if self.eat_kw("let") {
            let pat = self.pat()?;
            self.expect_sym("=")?;
            let value = Box::new(self.expr()?);
            self.expect_kw("then")?;
            let then = Box::new(self.expr()?);
            self.expect_kw("else")?;
            let other = Box::new(self.expr()?);
            return Ok(Expr::IfLet { pat, value, then, other });
        }
        let cond = Box::new(self.expr()?);
        self.expect_kw("then")?;
        let then = Box::new(self.expr()?);
        self.expect_kw("else")?;
        let other = Box::new(self.expr()?);
        Ok(Expr::If { cond, then, other })
    }

    // ------------------------------------------------------------------ forms
    fn form(&mut self, mut anns: Vec<Annotation>) -> Result<FormDecl, String> {
        let name = self.ident()?;
        self.expect_sym("(")?;
        let code = self.string()?;
        self.expect_sym(",")?;
        let title = self.string()?;
        self.expect_sym(")")?;
        self.expect_sym("{")?;
        self.expect_kw("columns")?;
        let mut columns = vec![self.ident()?];
        while self.eat_sym(",") {
            columns.push(self.ident()?);
        }
        self.expect_kw("input")?;
        let input = self.ident()?;
        let (mut items, mut checks, mut theorems) = (Vec::new(), Vec::new(), Vec::new());
        while !self.eat_sym("}") {
            let mut pre = self.annotations()?;
            let pos = self.pos();
            if self.eat_kw("item") {
                let id = self.string()?;
                let label = self.string()?;
                self.expect_sym("{")?;
                let mut cells = Vec::new();
                while !self.eat_sym("}") {
                    let col = self.ident()?;
                    self.expect_sym("=")?;
                    let def = if self.is_kw("input") {
                        self.i += 1;
                        let default = if self.is_sym("(") && self.same_line() {
                            let args = self.args()?;
                            match args.as_slice() {
                                [Arg { name: Some(n), value }] if n == "default" => Some(value.clone()),
                                _ => return Err(format!("{pos}: expected `input(default: <number>)`")),
                            }
                        } else {
                            None
                        };
                        CellDef::Input(default)
                    } else {
                        CellDef::Expr(self.expr()?)
                    };
                    cells.push((col, def));
                }
                pre.extend(self.annotations()?);
                items.push(FormItem { id, label, cells, anns: pre, pos });
            } else if self.is_kw("check") || self.is_kw("theorem") {
                let is_check = self.eat_kw("check");
                if !is_check {
                    self.expect_kw("theorem")?;
                }
                let warn = is_check && self.eat_kw("warn");
                let id = self.string()?;
                let description = self.string()?;
                self.expect_sym(":")?;
                let cond = Self::unwrap_anns(self.expr()?, &mut pre);
                let c = FormCheck { warn, id, description, cond, anns: pre };
                if is_check { checks.push(c) } else { theorems.push(c) }
            } else {
                return Err(self.err("expected `item`, `check` or `theorem`"));
            }
        }
        anns.extend(self.annotations()?);
        Ok(FormDecl { name, code, title, columns, input, items, checks, theorems, anns })
    }
}

fn describe(t: &Tok) -> String {
    match t {
        Tok::Ident(s) => format!("`{s}`"),
        Tok::Num(n) => format!("number {n}"),
        Tok::Percent(n) => format!("{n}%"),
        Tok::Str(s) => format!("string {s:?}"),
        Tok::Cell(i, c) => format!("[{i}:{c}]"),
        Tok::Sym(s) => format!("`{s}`"),
        Tok::Eof => "end of file".into(),
    }
}
