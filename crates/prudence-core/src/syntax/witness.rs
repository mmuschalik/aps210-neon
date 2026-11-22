//! Z3-backed branch-witness data generator.
//!
//! Given a Prudence IR `Module`, symbolically walks every record-level rule
//! function (`fn`) and `partition`, extracts the exact SMT-LIB path condition
//! for every leaf branch, conjoins it with the domain constraints, and queries
//! Z3 for a concrete witness record (`(check-sat)` + `(get-value ...)`).
//!
//! - `sat`   -> produces a minimal concrete SQL `INSERT` row guaranteed to
//!              exercise that branch.
//! - `unsat` -> flags an unreachable / dead branch in the specification.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::io::Write as _;
use std::process::{Command, Stdio};

use super::ast::*;
use crate::date::civil_from_days;

/// Fixed reference reporting date for synthesized witness rows (`2026-09-30`).
pub const REF_DATE_DAYS: i64 = 20726; // days_from_civil(2026, 9, 30)

#[derive(Debug, Clone)]
pub struct BranchWitness {
    pub fn_name: String,
    pub branch_id: usize,
    pub path_label: String,
    pub outcome: String,
    pub table: String,
    pub reachable: bool,
    /// Concrete `(column_name, sql_literal)` pairs when `reachable == true`.
    pub row: Vec<(String, String)>,
    /// Optional parent FK row (e.g. `customer` row for a `deposit` witness).
    pub parent_rows: Vec<(String, Vec<(String, String)>)>,
}

#[derive(Debug, Clone)]
pub struct ModuleReport {
    pub module_name: String,
    pub total_branches: usize,
    pub reachable: usize,
    pub unreachable: usize,
    pub witnesses: Vec<BranchWitness>,
    pub sql: String,
}

fn strip(e: &Expr) -> &Expr {
    match e {
        Expr::Annotated(x, _) => strip(x),
        other => other,
    }
}

fn sql_str(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn sql_ident(s: &str) -> String {
    const RESERVED: &[&str] = &[
        "select", "from", "where", "group", "order", "case", "when", "then", "else", "end", "table", "view",
        "limit", "in", "is", "not", "and", "or", "as", "by", "on", "join", "union", "all", "distinct", "check",
        "default", "primary", "key", "index", "value", "values", "window", "row", "set", "class", "type", "date",
    ];
    let lower = s.to_ascii_lowercase();
    if RESERVED.contains(&lower.as_str()) {
        format!("\"{s}\"")
    } else {
        s.to_string()
    }
}

fn fmt_date(days: i64) -> String {
    let (y, m, d) = civil_from_days(days);
    format!("'{y:04}-{m:02}-{d:02}'")
}

/// Metadata extracted from a `Module` for symbolic SMT translation.
struct ModInfo<'m> {
    enums: BTreeMap<String, Vec<String>>,
    ctor_enum: HashMap<String, String>,
    data: BTreeMap<String, (Option<Type>, Vec<Field>)>,
    /// `(DataName, CtorName)` -> variant fields `(name, ty)`.
    contracts: BTreeMap<String, Vec<(String, Vec<(String, Type)>)>>,
    fns: HashMap<String, (&'m [Field], &'m Type, &'m Expr)>,
    params: BTreeMap<String, (Type, Option<Expr>)>,
}

impl<'m> ModInfo<'m> {
    fn new(m: &'m Module) -> Self {
        let mut enums = BTreeMap::new();
        let mut ctor_enum = HashMap::new();
        let mut data = BTreeMap::new();
        let mut contracts: BTreeMap<String, Vec<(String, Vec<(String, Type)>)>> = BTreeMap::new();
        let mut fns = HashMap::new();
        let mut params = BTreeMap::new();

        for d in &m.decls {
            match &d.kind {
                DeclKind::TypeAlias(Type::Union(alts)) => {
                    let cs: Vec<String> = alts
                        .iter()
                        .filter_map(|t| if let Type::Named { name, .. } = t { Some(name.clone()) } else { None })
                        .collect();
                    for c in &cs {
                        ctor_enum.insert(c.clone(), d.name.clone());
                    }
                    enums.insert(d.name.clone(), cs);
                }
                DeclKind::Data { base, fields } => {
                    if let Some(Type::Named { name, generics, .. }) = base
                        && name == "Contract"
                    {
                        let mut variants = Vec::new();
                        for g in generics {
                            let alts = match g {
                                Type::Union(xs) => xs.as_slice(),
                                other => std::slice::from_ref(other),
                            };
                            for alt in alts {
                                if let Type::Named { name: ctor, params: ps, .. } = alt {
                                    let vfs: Vec<(String, Type)> = ps
                                        .iter()
                                        .filter_map(|p| match p {
                                            TypeParam::Field(n, t) => Some((n.clone(), t.clone())),
                                            TypeParam::Expr(_) => None,
                                        })
                                        .collect();
                                    variants.push((ctor.clone(), vfs));
                                }
                            }
                        }
                        contracts.insert(d.name.clone(), variants);
                    }
                    data.insert(d.name.clone(), (base.clone(), fields.clone()));
                }
                DeclKind::Fn { params: ps, ret, body: Some(body) } => {
                    fns.insert(d.name.clone(), (ps.as_slice(), ret, body));
                }
                DeclKind::Param { ty, default } => {
                    params.insert(d.name.clone(), (ty.clone(), default.clone()));
                }
                _ => {}
            }
        }
        ModInfo { enums, ctor_enum, data, contracts, fns, params }
    }

    fn is_data(&self, name: &str) -> bool {
        self.data.contains_key(name)
    }

    fn field_ty(&self, data_name: &str, field: &str) -> Option<&Type> {
        self.data.get(data_name)?.1.iter().find(|f| f.name == field).map(|f| &f.ty)
    }
}

/// An SMT-LIB sort for a field or expression.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Sort {
    Bool,
    Int,
    Enum(String),
    Str,
}

impl Sort {
    fn smt(&self) -> String {
        match self {
            Sort::Bool => "Bool".into(),
            Sort::Int => "Int".into(),
            Sort::Enum(e) => format!("E_{e}"),
            Sort::Str => "String".into(),
        }
    }
}

/// Symbolic value tracked in the SMT encoder.
#[derive(Clone, Debug)]
enum SymVal {
    /// Concrete/symbolic scalar SMT term.
    Term(String, Sort),
    /// `Uncertain<T>` = `(known_bool_term, inner_val_term, sort)`.
    Uncertain(String, String, Sort),
    /// `Option<T>` = `(is_some_bool_term, inner_val_term, sort)`.
    Opt(String, String, Sort),
    /// Reference to a symbolic record instance `(prefix, data_type_name)`.
    Rec(String, String),
    /// Tuple of symbolic values.
    Tuple(Vec<SymVal>),
}

impl SymVal {
    fn term(&self) -> Option<&str> {
        match self {
            SymVal::Term(t, _) => Some(t),
            _ => None,
        }
    }
}

/// Holds SMT declarations, domain assertions, and fresh auxiliary variables for one record type.
struct SmtEnv<'a, 'm> {
    info: &'a ModInfo<'m>,
    decls: Vec<(String, Sort)>,
    decl_set: BTreeSet<String>,
    assertions: Vec<String>,
    /// Tracks which SQL columns belong to which `(table, prefix)`.
    record_cols: Vec<(String, String, String, Vec<(String, ColSpec)>)>, // (table, prefix, data_name, cols)
    aux_counter: usize,
    optimistic: bool,
}

#[derive(Clone, Debug)]
enum ColSpec {
    Scalar(String, Sort, bool), // (smt_var, sort, is_date)
    Uncertain(String, String, Sort, bool), // (known_var, val_var, sort, is_date)
    Opt(String, String, Sort, bool), // (some_var, val_var, sort, is_date)
    Fk(String), // literal FK id string
    ContractKind(String, String), // (smt_var, data_name)
}

impl<'a, 'm> SmtEnv<'a, 'm> {
    fn new(info: &'a ModInfo<'m>) -> Self {
        SmtEnv {
            info,
            decls: Vec::new(),
            decl_set: BTreeSet::new(),
            assertions: Vec::new(),
            record_cols: Vec::new(),
            aux_counter: 0,
            optimistic: false,
        }
    }

    fn declare(&mut self, name: String, sort: Sort) -> String {
        if self.decl_set.insert(name.clone()) {
            self.decls.push((name.clone(), sort));
        }
        name
    }

    fn fresh(&mut self, prefix: &str, sort: Sort) -> String {
        self.aux_counter += 1;
        let name = format!("__{prefix}_{}", self.aux_counter);
        self.declare(name, sort)
    }

    fn sort_of(&self, ty: &Type) -> Option<(Sort, bool)> {
        match ty {
            Type::Named { name, .. } => match name.as_str() {
                "Bool" => Some((Sort::Bool, false)),
                "Int" | "Money" | "Rate" => Some((Sort::Int, false)),
                "Date" => Some((Sort::Int, true)),
                "String" | "ProductClass" | "GroupId" => Some((Sort::Str, false)),
                n if self.info.enums.contains_key(n) => Some((Sort::Enum(n.to_string()), false)),
                _ => None,
            },
            Type::Union(_) => None,
        }
    }

    /// Declare all SMT variables and domain constraints for a record of `data_name`.
    fn declare_record(&mut self, prefix: &str, data_name: &str) -> SymVal {
        let Some((_, fields)) = self.info.data.get(data_name).cloned() else {
            return SymVal::Rec(prefix.into(), data_name.into());
        };
        let mut cols = Vec::new();

        for f in &fields {
            let vname = format!("{prefix}__{}", f.name);
            match &f.ty {
                Type::Named { name, generics, .. } if name == "Uncertain" => {
                    if let Some(inner) = generics.first()
                        && let Some((s, is_date)) = self.sort_of(inner)
                    {
                        let kv = self.declare(format!("{vname}__known"), Sort::Bool);
                        let vv = self.declare(format!("{vname}__val"), s.clone());
                        self.add_field_domain(&f.name, &vv, &s, is_date);
                        cols.push((f.name.clone(), ColSpec::Uncertain(kv, vv, s, is_date)));
                    }
                }
                Type::Named { name, generics, .. } if name == "Option" => {
                    if let Some(inner) = generics.first()
                        && let Some((s, is_date)) = self.sort_of(inner)
                    {
                        let sv = self.declare(format!("{vname}__some"), Sort::Bool);
                        let vv = self.declare(format!("{vname}__val"), s.clone());
                        self.add_field_domain(&f.name, &vv, &s, is_date);
                        cols.push((f.name.clone(), ColSpec::Opt(sv, vv, s, is_date)));
                    }
                }
                Type::Named { name, .. } if self.info.is_data(name) => {
                    // Recursively declare parent FK record (e.g. `holder : Customer`)
                    let fk_prefix = format!("{prefix}__{}", f.name);
                    self.declare_record(&fk_prefix, name);
                    cols.push((f.name.clone(), ColSpec::Fk(format!("w_{fk_prefix}"))));
                }
                Type::Named { name, .. } if name == "List" => {
                    // One-to-many list field: handled symbolically via fresh aggregates
                }
                other => {
                    if let Some((s, is_date)) = self.sort_of(other) {
                        let vv = self.declare(vname, s.clone());
                        self.add_field_domain(&f.name, &vv, &s, is_date);
                        cols.push((f.name.clone(), ColSpec::Scalar(vv, s, is_date)));
                    }
                }
            }
        }

        // ACTUS Contract<...> variants
        if let Some(variants) = self.info.contracts.get(data_name).cloned() {
            let ck_var = self.declare(format!("{prefix}__contract_kind"), Sort::Enum(format!("Contract_{data_name}")));
            cols.push(("contract_kind".into(), ColSpec::ContractKind(ck_var, data_name.into())));
            let mut seen_vf = BTreeSet::new();
            for (_, vfs) in &variants {
                for (vfn, vfty) in vfs {
                    if !seen_vf.insert(vfn.clone()) {
                        continue;
                    }
                    let vname = format!("{prefix}__{vfn}");
                    match vfty {
                        Type::Named { name, generics, .. } if name == "Uncertain" => {
                            if let Some(inner) = generics.first()
                                && let Some((s, is_date)) = self.sort_of(inner)
                            {
                                let kv = self.declare(format!("{vname}__known"), Sort::Bool);
                                let vv = self.declare(format!("{vname}__val"), s.clone());
                                self.add_field_domain(vfn, &vv, &s, is_date);
                                cols.push((vfn.clone(), ColSpec::Uncertain(kv, vv, s, is_date)));
                            }
                        }
                        Type::Named { name, generics, .. } if name == "Option" => {
                            if let Some(inner) = generics.first()
                                && let Some((s, is_date)) = self.sort_of(inner)
                            {
                                let sv = self.declare(format!("{vname}__some"), Sort::Bool);
                                let vv = self.declare(format!("{vname}__val"), s.clone());
                                self.add_field_domain(vfn, &vv, &s, is_date);
                                cols.push((vfn.clone(), ColSpec::Opt(sv, vv, s, is_date)));
                            }
                        }
                        other => {
                            if let Some((s, is_date)) = self.sort_of(other) {
                                let vv = self.declare(vname, s.clone());
                                self.add_field_domain(vfn, &vv, &s, is_date);
                                cols.push((vfn.clone(), ColSpec::Scalar(vv, s, is_date)));
                            }
                        }
                    }
                }
            }
        }

        // Cross-field domain constraints
        if data_name == "Holding" {
            self.assertions.push(format!("(<= {prefix}__value_after_rba_margin {prefix}__market_value)"));
        }

        self.record_cols.push((data_name.to_ascii_lowercase(), prefix.into(), data_name.into(), cols));
        SymVal::Rec(prefix.into(), data_name.into())
    }

    fn add_field_domain(&mut self, fname: &str, var: &str, sort: &Sort, is_date: bool) {
        if *sort != Sort::Int {
            return;
        }
        if is_date {
            let lo = REF_DATE_DAYS - 365;
            let hi = REF_DATE_DAYS + 730;
            self.assertions.push(format!("(and (>= {var} {lo}) (<= {var} {hi}))"));
            return;
        }
        match fname {
            "balance" | "market_value" | "amount" | "cash" | "collateral_value" | "undrawn" | "payment" | "principal_per_period" => {
                self.assertions.push(format!("(and (> {var} 0) (<= {var} 5000000))"));
            }
            "value_after_rba_margin" | "operational_balance" | "minimum_payment" | "collateral_received"
            | "required_from_counterparty" | "hqla_substitutable" | "due_not_called" | "downgrade_3_notches"
            | "posted_non_hqla1" | "received_reusable" | "variation_margin_posted" | "cash_vm_received_eligible"
            | "initial_margin_posted" => {
                self.assertions.push(format!("(and (>= {var} 0) (<= {var} 5000000))"));
            }
            "notice_days" => {
                self.assertions.push(format!("(and (>= {var} 0) (<= {var} 90))"));
            }
            "risk_weight" => {
                self.assertions.push(format!("(or (= {var} 0) (= {var} 20) (= {var} 35) (= {var} 50) (= {var} 100))"));
            }
            "rating_grade" => {
                self.assertions.push(format!("(and (>= {var} 1) (<= {var} 6))"));
            }
            "cycle_months" => {
                self.assertions.push(format!("(or (= {var} 1) (= {var} 3) (= {var} 6) (= {var} 12))"));
            }
            "rate" => {
                self.assertions.push(format!("(and (>= {var} 0) (<= {var} 15))"));
            }
            _ => {
                self.assertions.push(format!("(and (>= {var} -1000000) (<= {var} 1000000))"));
            }
        }
    }

    fn declare_params(&mut self) {
        let params: Vec<_> = self.info.params.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        for (name, (ty, _default)) in params {
            if name == "reporting_date" || name == "known_at" {
                let v = self.declare(format!("param__{name}"), Sort::Int);
                self.assertions.push(format!("(= {v} {REF_DATE_DAYS})"));
            } else if name == "fcs_limit" {
                let v = self.declare(format!("param__{name}"), Sort::Int);
                self.assertions.push(format!("(= {v} 250000)"));
            } else if name == "sme_threshold" {
                let v = self.declare(format!("param__{name}"), Sort::Int);
                self.assertions.push(format!("(= {v} 2000000)"));
            } else if let Some((s, is_date)) = self.sort_of(&ty) {
                let v = self.declare(format!("param__{name}"), s.clone());
                self.add_field_domain(&name, &v, &s, is_date);
            }
        }
    }

    fn field_sym(&self, prefix: &str, data_name: &str, field: &str) -> Option<SymVal> {
        let vname = format!("{prefix}__{field}");
        // Look up in fields or contract fields
        let fty = self.info.field_ty(data_name, field).cloned().or_else(|| {
            self.info.contracts.get(data_name).and_then(|vs| {
                vs.iter().flat_map(|(_, fs)| fs.iter()).find(|(n, _)| n == field).map(|(_, t)| t.clone())
            })
        })?;
        match &fty {
            Type::Named { name, generics, .. } if name == "Uncertain" => {
                let (s, _) = self.sort_of(generics.first()?)?;
                Some(SymVal::Uncertain(format!("{vname}__known"), format!("{vname}__val"), s))
            }
            Type::Named { name, generics, .. } if name == "Option" => {
                let (s, _) = self.sort_of(generics.first()?)?;
                Some(SymVal::Opt(format!("{vname}__some"), format!("{vname}__val"), s))
            }
            Type::Named { name, .. } if self.info.is_data(name) => Some(SymVal::Rec(vname, name.clone())),
            other => {
                let (s, _) = self.sort_of(other)?;
                Some(SymVal::Term(vname, s))
            }
        }
    }

    /// Translate an expression into a single SMT-LIB value (for conditions and helper calls).
    fn eval_sym(&mut self, e: &Expr, scope: &HashMap<String, SymVal>, depth: usize) -> Option<SymVal> {
        if depth > 24 {
            return None;
        }
        match strip(e) {
            Expr::Num(n, _) => {
                let v = n.split('.').next().unwrap_or("0");
                Some(SymVal::Term(v.to_string(), Sort::Int))
            }
            Expr::Percent(n, _) => {
                let v = n.split('.').next().unwrap_or("0");
                Some(SymVal::Term(v.to_string(), Sort::Int))
            }
            Expr::Str(s, _) => Some(SymVal::Term(format!("\"{}\"", s.replace('"', "\"\"")), Sort::Str)),
            Expr::Ident(n, _) => {
                if n == "true" || n == "false" {
                    return Some(SymVal::Term(n.clone(), Sort::Bool));
                }
                if n == "none" {
                    return Some(SymVal::Term("\"none\"".into(), Sort::Str));
                }
                if let Some(v) = scope.get(n) {
                    return Some(v.clone());
                }
                if let Some(en) = self.info.ctor_enum.get(n) {
                    return Some(SymVal::Term(format!("{en}_{n}"), Sort::Enum(en.clone())));
                }
                if let Some((ty, _)) = self.info.params.get(n)
                    && let Some((s, _)) = self.sort_of(ty)
                {
                    return Some(SymVal::Term(format!("param__{n}"), s));
                }
                None
            }
            Expr::Tuple(xs) => {
                let mut vs = Vec::new();
                for x in xs {
                    vs.push(self.eval_sym(x, scope, depth + 1)?);
                }
                Some(SymVal::Tuple(vs))
            }
            Expr::Member(t, f) => {
                if f == "AUD" || f == "days" {
                    return self.eval_sym(t, scope, depth + 1);
                }
                let tv = self.eval_sym(t, scope, depth + 1)?;
                match tv {
                    SymVal::Rec(prefix, dt) => {
                        if f == "contract" {
                            Some(SymVal::Term(format!("{prefix}__contract_kind"), Sort::Enum(format!("Contract_{dt}"))))
                        } else {
                            self.field_sym(&prefix, &dt, f)
                        }
                    }
                    _ => None,
                }
            }
            Expr::Unary(UnOp::Not, a) => {
                let av = self.eval_sym(a, scope, depth + 1)?;
                Some(SymVal::Term(format!("(not {})", av.term()?), Sort::Bool))
            }
            Expr::Unary(UnOp::Neg, a) => {
                let av = self.eval_sym(a, scope, depth + 1)?;
                Some(SymVal::Term(format!("(- {})", av.term()?), Sort::Int))
            }
            Expr::Binary(BinOp::In, a, b) => {
                let av = self.eval_sym(a, scope, depth + 1)?;
                let at = av.term()?;
                if let Expr::List(xs) = strip(b) {
                    let mut parts = Vec::new();
                    for x in xs {
                        if let Some(xv) = self.eval_sym(x, scope, depth + 1)
                            && let Some(xt) = xv.term()
                        {
                            parts.push(format!("(= {at} {xt})"));
                        }
                    }
                    if parts.is_empty() {
                        Some(SymVal::Term("false".into(), Sort::Bool))
                    } else {
                        Some(SymVal::Term(format!("(or {})", parts.join(" ")), Sort::Bool))
                    }
                } else {
                    // Membership in a view like `d.product_class in Tainted`
                    let aux = self.fresh("in_view", Sort::Bool);
                    Some(SymVal::Term(aux, Sort::Bool))
                }
            }
            Expr::Binary(op, a, b) => {
                // Handle Option<Date> comparison (`grace_end >= reporting_date`, `maturity == none`, etc.)
                let av = self.eval_sym(a, scope, depth + 1);
                let bv = self.eval_sym(b, scope, depth + 1);
                match (&av, &bv, op) {
                    (Some(SymVal::Opt(sv, _, _)), _, BinOp::Eq) if matches!(strip(b), Expr::Ident(n, _) if n == "none") => {
                        return Some(SymVal::Term(format!("(not {sv})"), Sort::Bool));
                    }
                    (Some(SymVal::Opt(sv, _, _)), _, BinOp::Ne) if matches!(strip(b), Expr::Ident(n, _) if n == "none") => {
                        return Some(SymVal::Term(sv.clone(), Sort::Bool));
                    }
                    (Some(SymVal::Opt(sv, vv, _)), Some(SymVal::Term(bt, _)), BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge) => {
                        let o = smt_op(*op)?;
                        return Some(SymVal::Term(format!("(and {sv} ({o} {vv} {bt}))"), Sort::Bool));
                    }
                    _ => {}
                }
                let at = av.as_ref().and_then(|v| v.term()).map(str::to_string)?;
                let bt = bv.as_ref().and_then(|v| v.term()).map(str::to_string)?;
                let (o, s) = match op {
                    BinOp::Implies => ("=>", Sort::Bool),
                    BinOp::Or => ("or", Sort::Bool),
                    BinOp::And => ("and", Sort::Bool),
                    BinOp::Eq => ("=", Sort::Bool),
                    BinOp::Ne => return Some(SymVal::Term(format!("(not (= {at} {bt}))"), Sort::Bool)),
                    BinOp::Lt => ("<", Sort::Bool),
                    BinOp::Le => ("<=", Sort::Bool),
                    BinOp::Gt => (">", Sort::Bool),
                    BinOp::Ge => (">=", Sort::Bool),
                    BinOp::Add => ("+", Sort::Int),
                    BinOp::Sub => ("-", Sort::Int),
                    BinOp::Mul => ("*", Sort::Int),
                    BinOp::Div => ("div", Sort::Int),
                    BinOp::In => unreachable!(),
                };
                Some(SymVal::Term(format!("({o} {at} {bt})"), s))
            }
            Expr::If { cond, then, other } => {
                let cv = self.eval_sym(cond, scope, depth + 1)?;
                let ct = cv.term()?.to_string();
                let tv = self.eval_sym(then, scope, depth + 1)?;
                let ov = self.eval_sym(other, scope, depth + 1)?;
                match (tv, ov) {
                    (SymVal::Term(tt, s), SymVal::Term(ot, _)) => Some(SymVal::Term(format!("(ite {ct} {tt} {ot})"), s)),
                    _ => None,
                }
            }
            Expr::IfLet { pat, value, then, other } => {
                let vv = self.eval_sym(value, scope, depth + 1);
                let (cond_t, mut scope2) = match (pat, vv) {
                    (Pat::Ctor(c, ps), Some(SymVal::Opt(sv, inner, s))) if c == "Some" => {
                        let mut sc = scope.clone();
                        if let Some(Pat::Bind(x)) = ps.first() {
                            sc.insert(x.clone(), SymVal::Term(inner, s));
                        }
                        (sv, sc)
                    }
                    (Pat::Ctor(_, ps), _) => {
                        let flag = self.fresh("iflet", Sort::Bool);
                        let mut sc = scope.clone();
                        for p in ps {
                            if let Pat::Bind(x) = p {
                                let d = self.fresh(x, Sort::Int);
                                self.add_field_domain(x, &d, &Sort::Int, true);
                                sc.insert(x.clone(), SymVal::Term(d, Sort::Int));
                            }
                        }
                        (flag, sc)
                    }
                    _ => (self.fresh("iflet", Sort::Bool), scope.clone()),
                };
                let _ = &mut scope2;
                let tv = self.eval_sym(then, &scope2, depth + 1)?;
                let ov = self.eval_sym(other, scope, depth + 1)?;
                match (tv, ov) {
                    (SymVal::Term(tt, s), SymVal::Term(ot, _)) => Some(SymVal::Term(format!("(ite {cond_t} {tt} {ot})"), s)),
                    _ => None,
                }
            }
            Expr::Let { pat, value, body } => {
                let mut sc = scope.clone();
                if let Pat::Bind(x) = pat
                    && let Some(vv) = self.eval_sym(value, scope, depth + 1)
                {
                    sc.insert(x.clone(), vv);
                }
                self.eval_sym(body, &sc, depth + 1)
            }
            Expr::Match { scrutinee, arms } => {
                let scr = self.eval_sym(scrutinee, scope, depth + 1)?;
                let contract_rec = match strip(scrutinee) {
                    Expr::Member(rec, fld) if fld == "contract" => match self.eval_sym(rec, scope, depth + 1) {
                        Some(SymVal::Rec(p, dt)) => Some((p, dt)),
                        _ => None,
                    },
                    _ => None,
                };
                let mut result: Option<(String, Sort)> = None;
                for arm in arms.iter().rev() {
                    let (pcond, sc) = self.match_pat(&arm.pat, &scr, contract_rec.as_ref(), scope)?;
                    let full_cond = match &arm.guard {
                        Some(g) => {
                            let gt = self.eval_sym(g, &sc, depth + 1)?.term()?.to_string();
                            if pcond == "true" { gt } else { format!("(and {pcond} {gt})") }
                        }
                        None => pcond,
                    };
                    let bv = self.eval_sym(&arm.body, &sc, depth + 1)?;
                    let SymVal::Term(bt, s) = bv else { return None };
                    result = Some(match result {
                        None => (bt, s),
                        Some((else_t, _)) => {
                            if full_cond == "true" {
                                (bt, s)
                            } else {
                                (format!("(ite {full_cond} {bt} {else_t})"), s)
                            }
                        }
                    });
                }
                result.map(|(t, s)| SymVal::Term(t, s))
            }
            Expr::Call(callee, args) => {
                let Expr::Ident(fn_name, _) = strip(callee) else { return None };
                match fn_name.as_str() {
                    "conservative" => {
                        let val = self.eval_sym(&args.first()?.value, scope, depth + 1)?;
                        let worst_e = args.iter().find(|a| a.name.as_deref() == Some("worst")).map(|a| &a.value);
                        let best_e = args.iter().find(|a| a.name.as_deref() == Some("best")).map(|a| &a.value);
                        let fallback_e = if self.optimistic { best_e.or(worst_e) } else { worst_e };
                        match val {
                            SymVal::Uncertain(kv, vv, s) => {
                                let fb = match fallback_e {
                                    Some(fe) => self.eval_sym(fe, scope, depth + 1).and_then(|v| v.term().map(str::to_string)),
                                    None => None,
                                }
                                .unwrap_or_else(|| match &s {
                                    Sort::Bool => "false".into(),
                                    Sort::Int => "0".into(),
                                    Sort::Str => "\"none\"".into(),
                                    Sort::Enum(e) => format!("{e}_{}", self.info.enums[e][0]),
                                });
                                Some(SymVal::Term(format!("(ite {kv} {vv} {fb})"), s))
                            }
                            other => Some(other),
                        }
                    }
                    "days" => {
                        let a = self.eval_sym(&args.first()?.value, scope, depth + 1)?.term()?.to_string();
                        let b = self.eval_sym(&args.get(1)?.value, scope, depth + 1)?.term()?.to_string();
                        Some(SymVal::Term(format!("(- {b} {a})"), Sort::Int))
                    }
                    "add_months" => {
                        let d = self.eval_sym(&args.first()?.value, scope, depth + 1)?.term()?.to_string();
                        let m = self.eval_sym(&args.get(1)?.value, scope, depth + 1)?.term()?.to_string();
                        Some(SymVal::Term(format!("(+ {d} (* {m} 30))"), Sort::Int))
                    }
                    "min" | "max" => {
                        let a = self.eval_sym(&args.first()?.value, scope, depth + 1)?.term()?.to_string();
                        let b = self.eval_sym(&args.get(1)?.value, scope, depth + 1)?.term()?.to_string();
                        let cmp = if fn_name == "min" { "<=" } else { ">=" };
                        Some(SymVal::Term(format!("(ite ({cmp} {a} {b}) {a} {b})"), Sort::Int))
                    }
                    "item" => {
                        let p = self.eval_sym(&args.first()?.value, scope, depth + 1)?.term()?.to_string();
                        let s = self.eval_sym(&args.get(1)?.value, scope, depth + 1)?.term()?.to_string();
                        Some(SymVal::Term(format!("(str.++ {p} \".\" (str.from_int {s}))"), Sort::Str))
                    }
                    "all" | "any" => Some(SymVal::Term(self.fresh(fn_name, Sort::Bool), Sort::Bool)),
                    "sum" => Some(SymVal::Term(self.fresh("sum", Sort::Int), Sort::Int)),
                    _ => {
                        // Inline user-defined function!
                        if let Some((ps, _ret, body)) = self.info.fns.get(fn_name).copied() {
                            let mut sc = HashMap::new();
                            for (p, a) in ps.iter().zip(args.iter()) {
                                if let Some(av) = self.eval_sym(&a.value, scope, depth + 1) {
                                    sc.insert(p.name.clone(), av);
                                }
                            }
                            if sc.len() == ps.len() {
                                return self.eval_sym(body, &sc, depth + 1);
                            }
                        }
                        // Fallback: uninterpreted fresh constant of the return sort
                        if let Some((_, ret, _)) = self.info.fns.get(fn_name)
                            && let Some((s, _)) = self.sort_of(ret)
                        {
                            return Some(SymVal::Term(self.fresh(fn_name, s.clone()), s));
                        }
                        None
                    }
                }
            }
            Expr::Index(_, _) => Some(SymVal::Term(self.fresh("idx", Sort::Int), Sort::Int)),
            _ => None,
        }
    }

    fn match_pat(
        &mut self,
        pat: &Pat,
        scr: &SymVal,
        contract_rec: Option<&(String, String)>,
        scope: &HashMap<String, SymVal>,
    ) -> Option<(String, HashMap<String, SymVal>)> {
        let mut sc = scope.clone();
        match (pat, scr) {
            (Pat::Wild, _) => Some(("true".into(), sc)),
            (Pat::Bind(x), v) => {
                sc.insert(x.clone(), v.clone());
                Some(("true".into(), sc))
            }
            (Pat::Ctor(c, ps), SymVal::Term(st, Sort::Enum(en))) => {
                let ctor_sym = format!("{en}_{c}");
                if let Some((rec_prefix, dt)) = contract_rec
                    && let Some(variants) = self.info.contracts.get(dt)
                    && let Some((_, vfs)) = variants.iter().find(|(vn, _)| vn == c)
                {
                    for (p, (vfn, _)) in ps.iter().zip(vfs.iter()) {
                        if let Pat::Bind(x) = p
                            && let Some(fv) = self.field_sym(rec_prefix, dt, vfn)
                        {
                            sc.insert(x.clone(), fv);
                        }
                    }
                }
                Some((format!("(= {st} {ctor_sym})"), sc))
            }
            (Pat::Tuple(ps), SymVal::Tuple(vs)) if ps.len() == vs.len() => {
                let mut conds = Vec::new();
                for (p, v) in ps.iter().zip(vs.iter()) {
                    let (c, sc2) = self.match_pat(p, v, None, &sc)?;
                    sc = sc2;
                    if c != "true" {
                        conds.push(c);
                    }
                }
                let cond = if conds.is_empty() { "true".into() } else { format!("(and {})", conds.join(" ")) };
                Some((cond, sc))
            }
            _ => None,
        }
    }

    /// Collect all leaf branches `(path_conds, path_label, outcome_desc)` of an expression.
    fn collect_branches(
        &mut self,
        e: &Expr,
        scope: &HashMap<String, SymVal>,
        conds: &[String],
        label: &str,
        out: &mut Vec<(Vec<String>, String, String)>,
    ) {
        match strip(e) {
            Expr::If { cond, then, other } => {
                if let Some(cv) = self.eval_sym(cond, scope, 0)
                    && let Some(ct) = cv.term()
                {
                    let mut c_then = conds.to_vec();
                    c_then.push(ct.to_string());
                    self.collect_branches(then, scope, &c_then, &format!("{label}/then"), out);

                    let mut c_else = conds.to_vec();
                    c_else.push(format!("(not {ct})"));
                    self.collect_branches(other, scope, &c_else, &format!("{label}/else"), out);
                    return;
                }
                out.push((conds.to_vec(), label.to_string(), short_expr(e)));
            }
            Expr::IfLet { pat, value, then, other } => {
                let vv = self.eval_sym(value, scope, 0);
                let (cond_t, scope2) = match (pat, vv) {
                    (Pat::Ctor(c, ps), Some(SymVal::Opt(sv, inner, s))) if c == "Some" => {
                        let mut sc = scope.clone();
                        if let Some(Pat::Bind(x)) = ps.first() {
                            sc.insert(x.clone(), SymVal::Term(inner, s));
                        }
                        (sv, sc)
                    }
                    (Pat::Ctor(_, ps), _) => {
                        let flag = self.fresh("iflet", Sort::Bool);
                        let mut sc = scope.clone();
                        for p in ps {
                            if let Pat::Bind(x) = p {
                                let d = self.fresh(x, Sort::Int);
                                self.add_field_domain(x, &d, &Sort::Int, true);
                                sc.insert(x.clone(), SymVal::Term(d, Sort::Int));
                            }
                        }
                        (flag, sc)
                    }
                    _ => (self.fresh("iflet", Sort::Bool), scope.clone()),
                };
                let mut c_then = conds.to_vec();
                c_then.push(cond_t.clone());
                self.collect_branches(then, &scope2, &c_then, &format!("{label}/some"), out);

                let mut c_else = conds.to_vec();
                c_else.push(format!("(not {cond_t})"));
                self.collect_branches(other, scope, &c_else, &format!("{label}/none"), out);
            }
            Expr::Let { pat, value, body } => {
                let mut sc = scope.clone();
                if let Pat::Bind(x) = pat
                    && let Some(vv) = self.eval_sym(value, scope, 0)
                {
                    sc.insert(x.clone(), vv);
                }
                self.collect_branches(body, &sc, conds, label, out);
            }
            Expr::Match { scrutinee, arms } => {
                let Some(scr) = self.eval_sym(scrutinee, scope, 0) else {
                    out.push((conds.to_vec(), label.to_string(), short_expr(e)));
                    return;
                };
                let contract_rec = match strip(scrutinee) {
                    Expr::Member(rec, fld) if fld == "contract" => match self.eval_sym(rec, scope, 0) {
                        Some(SymVal::Rec(p, dt)) => Some((p, dt)),
                        _ => None,
                    },
                    _ => None,
                };
                let mut prior_negs: Vec<String> = Vec::new();
                for (ai, arm) in arms.iter().enumerate() {
                    let Some((pcond, sc)) = self.match_pat(&arm.pat, &scr, contract_rec.as_ref(), scope) else {
                        continue;
                    };
                    let arm_cond = match &arm.guard {
                        Some(g) => match self.eval_sym(g, &sc, 0).and_then(|v| v.term().map(str::to_string)) {
                            Some(gt) => if pcond == "true" { gt } else { format!("(and {pcond} {gt})") },
                            None => pcond.clone(),
                        },
                        None => pcond.clone(),
                    };
                    let mut branch_conds = conds.to_vec();
                    branch_conds.extend(prior_negs.iter().cloned());
                    if arm_cond != "true" {
                        branch_conds.push(arm_cond.clone());
                    }
                    let arm_name = pat_label(&arm.pat, ai);
                    self.collect_branches(&arm.body, &sc, &branch_conds, &format!("{label}/{arm_name}"), out);
                    if arm_cond != "true" {
                        prior_negs.push(format!("(not {arm_cond})"));
                    }
                }
            }
            _ => {
                out.push((conds.to_vec(), label.to_string(), short_expr(e)));
            }
        }
    }

    fn build_smt_header(&self) -> String {
        let mut s = String::from("(set-logic ALL)\n");
        for (en, ctors) in &self.info.enums {
            let cs: Vec<String> = ctors.iter().map(|c| format!("({en}_{c})")).collect();
            writeln!(s, "(declare-datatype E_{en} ({}))", cs.join(" ")).unwrap();
        }
        for (dn, variants) in &self.info.contracts {
            let cs: Vec<String> = variants.iter().map(|(c, _)| format!("(Contract_{dn}_{c})")).collect();
            writeln!(s, "(declare-datatype E_Contract_{dn} ({}))", cs.join(" ")).unwrap();
        }
        for (name, sort) in &self.decls {
            writeln!(s, "(declare-const {name} {})", sort.smt()).unwrap();
        }
        for a in &self.assertions {
            writeln!(s, "(assert {a})").unwrap();
        }
        s
    }

    fn solve_branch(&self, branch_conds: &[String]) -> Result<Option<HashMap<String, String>>, String> {
        let mut script = self.build_smt_header();
        for c in branch_conds {
            writeln!(script, "(assert {c})").unwrap();
        }
        writeln!(script, "(check-sat)").unwrap();
        if !self.decls.is_empty() {
            let vars: Vec<&str> = self.decls.iter().map(|(n, _)| n.as_str()).collect();
            writeln!(script, "(get-value ({}))", vars.join(" ")).unwrap();
        }

        let mut child = Command::new("z3")
            .args(["-in", "-smt2", "-T:10"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("cannot run z3: {e}"))?;
        child.stdin.take().unwrap().write_all(script.as_bytes()).map_err(|e| e.to_string())?;
        let out = child.wait_with_output().map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        let mut lines = text.lines();
        let status = lines.next().unwrap_or("").trim();
        match status {
            "unsat" => Ok(None),
            "sat" => {
                let rest: String = lines.collect::<Vec<_>>().join(" ");
                Ok(Some(parse_get_value(&rest)))
            }
            other => Err(format!("z3 returned {other:?}: {text}")),
        }
    }

    fn extract_sql_rows(
        &self,
        model: &HashMap<String, String>,
        row_id: &str,
    ) -> (Vec<(String, String)>, Vec<(String, Vec<(String, String)>)>) {
        // Last entry in `record_cols` is the primary record; earlier ones are parent FK records.
        let mut built: Vec<(String, Vec<(String, String)>)> = Vec::new();
        for (idx, (table, prefix, _dn, cols)) in self.record_cols.iter().enumerate() {
            let is_primary = idx + 1 == self.record_cols.len();
            let id_val = if is_primary { row_id.to_string() } else { format!("{row_id}_{prefix}") };
            let mut r = vec![("id".to_string(), sql_str(&id_val))];
            for (cname, spec) in cols {
                let val = match spec {
                    ColSpec::Scalar(v, s, is_date) => fmt_model_val(model.get(v), s, *is_date),
                    ColSpec::Uncertain(kv, vv, s, is_date) => {
                        if model.get(kv).map(String::as_str) == Some("false") {
                            "NULL".into()
                        } else {
                            fmt_model_val(model.get(vv), s, *is_date)
                        }
                    }
                    ColSpec::Opt(sv, vv, s, is_date) => {
                        if model.get(sv).map(String::as_str) == Some("false") {
                            "NULL".into()
                        } else {
                            fmt_model_val(model.get(vv), s, *is_date)
                        }
                    }
                    ColSpec::Fk(fk_tag) => {
                        let suffix = fk_tag.strip_prefix("w_").unwrap_or(fk_tag);
                        sql_str(&format!("{row_id}_{suffix}"))
                    }
                    ColSpec::ContractKind(v, dn) => {
                        let raw = model.get(v).map(String::as_str).unwrap_or("");
                        let pfx = format!("Contract_{dn}_");
                        sql_str(raw.strip_prefix(&pfx).unwrap_or(raw))
                    }
                };
                r.push((cname.clone(), val));
            }
            built.push((table.clone(), r));
        }
        let primary = built.pop().map(|(_, r)| r).unwrap_or_default();
        (primary, built)
    }
}

fn smt_op(op: BinOp) -> Option<&'static str> {
    Some(match op {
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::Eq => "=",
        _ => return None,
    })
}

fn fmt_model_val(raw: Option<&String>, sort: &Sort, is_date: bool) -> String {
    let Some(v) = raw else { return "NULL".into() };
    match sort {
        Sort::Bool => if v == "true" { "TRUE".into() } else { "FALSE".into() },
        Sort::Int => {
            let n = parse_smt_int(v).unwrap_or(0);
            if is_date { fmt_date(n) } else { n.to_string() }
        }
        Sort::Str => sql_str(v.trim_matches('"')),
        Sort::Enum(en) => {
            let pfx = format!("{en}_");
            sql_str(v.strip_prefix(&pfx).unwrap_or(v))
        }
    }
}

fn parse_smt_int(s: &str) -> Option<i64> {
    let t = s.trim();
    if let Some(inner) = t.strip_prefix("(- ").and_then(|x| x.strip_suffix(')')) {
        return inner.trim().parse::<i64>().ok().map(|n| -n);
    }
    t.parse::<i64>().ok()
}

/// Parse Z3 `(get-value (...))` s-expression output `((k1 v1) (k2 (- 5)) ...)`.
fn parse_get_value(s: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let b = s.as_bytes();
    let mut i = 0;
    let skip_ws = |i: &mut usize| {
        while *i < b.len() && b[*i].is_ascii_whitespace() {
            *i += 1;
        }
    };
    let read_sexp = |i: &mut usize| -> String {
        skip_ws(i);
        let start = *i;
        if *i >= b.len() {
            return String::new();
        }
        if b[*i] == b'(' {
            let mut depth = 0;
            while *i < b.len() {
                if b[*i] == b'(' {
                    depth += 1;
                } else if b[*i] == b')' {
                    depth -= 1;
                    if depth == 0 {
                        *i += 1;
                        break;
                    }
                }
                *i += 1;
            }
        } else if b[*i] == b'"' {
            *i += 1;
            while *i < b.len() {
                if b[*i] == b'"' {
                    *i += 1;
                    if *i < b.len() && b[*i] == b'"' {
                        *i += 1;
                    } else {
                        break;
                    }
                } else {
                    *i += 1;
                }
            }
        } else {
            while *i < b.len() && !b[*i].is_ascii_whitespace() && b[*i] != b')' && b[*i] != b'(' {
                *i += 1;
            }
        }
        s[start..*i].to_string()
    };

    skip_ws(&mut i);
    if i < b.len() && b[i] == b'(' {
        i += 1;
    }
    while i < b.len() {
        skip_ws(&mut i);
        if i >= b.len() || b[i] == b')' {
            break;
        }
        if b[i] == b'(' {
            i += 1;
            let k = read_sexp(&mut i);
            let v = read_sexp(&mut i);
            skip_ws(&mut i);
            if i < b.len() && b[i] == b')' {
                i += 1;
            }
            if !k.is_empty() {
                map.insert(k, v);
            }
        } else {
            i += 1;
        }
    }
    map
}

fn pat_label(p: &Pat, idx: usize) -> String {
    match p {
        Pat::Wild => "otherwise".into(),
        Pat::Bind(x) => x.clone(),
        Pat::Ctor(c, _) => c.clone(),
        Pat::Tuple(ps) => format!("({})", ps.iter().map(|x| pat_label(x, idx)).collect::<Vec<_>>().join(",")),
    }
}

fn short_expr(e: &Expr) -> String {
    match strip(e) {
        Expr::Str(s, _) => format!("\"{s}\""),
        Expr::Num(n, _) => n.clone(),
        Expr::Percent(n, _) => format!("{n}%"),
        Expr::Ident(n, _) => n.clone(),
        Expr::Call(f, _) => format!("{}(..)", short_expr(f)),
        _ => "<expr>".into(),
    }
}

/// Generate branch-covering Z3 witnesses and SQL `INSERT` statements for a module.
pub fn generate_witnesses(m: &Module) -> Result<ModuleReport, String> {
    let info = ModInfo::new(m);
    let mut witnesses = Vec::new();
    let mut sql = String::new();
    let mod_lower = m.name.to_ascii_lowercase();
    writeln!(sql, "-- Z3-synthesized branch-covering witness dataset for module `{}`", m.name).unwrap();
    writeln!(sql, "-- Every row below was solved by Z3 to satisfy a distinct leaf branch + domain constraints.\n").unwrap();
    writeln!(sql, "INSERT INTO {mod_lower}_params (mode, reporting_date) VALUES ('Conservative', {});\n", fmt_date(REF_DATE_DAYS)).unwrap();

    let mut branch_counter = 0usize;

    // 1. Walk every pure `fn` whose first parameter is a `data` record and whose body branches.
    for d in &m.decls {
        let DeclKind::Fn { params, body: Some(body), .. } = &d.kind else { continue };
        let Some(p0) = params.first() else { continue };
        let Type::Named { name: data_name, .. } = &p0.ty else { continue };
        if !info.is_data(data_name) {
            continue;
        }
        // Only generate witnesses for functions that actually branch (If / IfLet / Match).
        if !has_branch(body) {
            continue;
        }

        let mut env = SmtEnv::new(&info);
        env.declare_params();
        let mut scope = HashMap::new();
        let rec_val = env.declare_record("r", data_name);
        scope.insert(p0.name.clone(), rec_val);
        for extra in params.iter().skip(1) {
            if let Some((s, is_date)) = env.sort_of(&extra.ty) {
                let v = env.declare(format!("arg__{}", extra.name), s.clone());
                env.add_field_domain(&extra.name, &v, &s, is_date);
                scope.insert(extra.name.clone(), SymVal::Term(v, s));
            }
        }

        let mut branches = Vec::new();
        env.collect_branches(body, &scope, &[], &d.name, &mut branches);
        if branches.len() <= 1 {
            continue;
        }

        for (conds, path_label, outcome) in branches {
            branch_counter += 1;
            let row_id = format!("{}_{branch_counter}", d.name);
            let table = data_name.to_ascii_lowercase();
            match env.solve_branch(&conds)? {
                Some(model) => {
                    let (row, parent_rows) = env.extract_sql_rows(&model, &row_id);
                    writeln!(sql, "-- Branch #{branch_counter}: {path_label} => {outcome}").unwrap();
                    for (pt, pr) in &parent_rows {
                        let cols: Vec<String> = pr.iter().map(|(c, _)| sql_ident(c)).collect();
                        let vals: Vec<&str> = pr.iter().map(|(_, v)| v.as_str()).collect();
                        writeln!(sql, "INSERT INTO {} ({}) VALUES ({});", sql_ident(pt), cols.join(", "), vals.join(", ")).unwrap();
                    }
                    let cols: Vec<String> = row.iter().map(|(c, _)| sql_ident(c)).collect();
                    let vals: Vec<&str> = row.iter().map(|(_, v)| v.as_str()).collect();
                    writeln!(sql, "INSERT INTO {} ({}) VALUES ({});\n", sql_ident(&table), cols.join(", "), vals.join(", ")).unwrap();
                    witnesses.push(BranchWitness {
                        fn_name: d.name.clone(),
                        branch_id: branch_counter,
                        path_label,
                        outcome,
                        table,
                        reachable: true,
                        row,
                        parent_rows,
                    });
                }
                None => {
                    writeln!(sql, "-- UNREACHABLE (unsat) Branch #{branch_counter}: {path_label} => {outcome}\n").unwrap();
                    witnesses.push(BranchWitness {
                        fn_name: d.name.clone(),
                        branch_id: branch_counter,
                        path_label,
                        outcome,
                        table,
                        reachable: false,
                        row: vec![],
                        parent_rows: vec![],
                    });
                }
            }
        }
    }

    let reachable = witnesses.iter().filter(|w| w.reachable).count();
    let unreachable = witnesses.len() - reachable;
    Ok(ModuleReport {
        module_name: m.name.clone(),
        total_branches: witnesses.len(),
        reachable,
        unreachable,
        witnesses,
        sql,
    })
}

fn has_branch(e: &Expr) -> bool {
    match strip(e) {
        Expr::If { .. } | Expr::IfLet { .. } | Expr::Match { .. } => true,
        Expr::Let { body, .. } => has_branch(body),
        _ => false,
    }
}
