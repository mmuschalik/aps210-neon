import Prudence.DSL

/-!
# Prudence IR -> DuckDB SQL Compiler

Generates directly executable DuckDB SQL from `Prudence.DSL.FormDecl` and `Prudence.DSL.Module`.

- **Forms (`FormDecl`)**: Topologically layered CTEs over `<form>_inputs`, unpivoted
  `<form>_cells` view, and `<form>_checks` validation view.
- **Modules (`Module`)**:
  - Parameter table `<mod>_params` + `_p_mode()` / `_p_<param>()` scalar macros.
  - Built-in DuckDB helper macros (`bucket(d)`, `solo(c)`).
  - `data` / `event` schemas as `CREATE TABLE IF NOT EXISTS` DDL (ACTUS contracts
    flattened to `contract_kind` + nullable variant columns).
  - `alloc ... = optimize` blocks as external input tables `alloc_<name>` (solved
    outside SQL by `prudence-core`'s exact simplex with primal-dual certificates).
  - `view`s as `CREATE OR REPLACE VIEW` (`GROUP BY`, `SELECT DISTINCT`, bitemporal
    `valid <= _p_reporting_date() AND recorded <= _p_known_at()`).
  - Pure `fn`s as DuckDB scalar macros (`CREATE OR REPLACE MACRO`) or table macros
    (`AS TABLE`) for list-returning functions (`realised`, `window_ends`), with full
    lowering of FK joins (`d.holder.kind`), one-to-many collections (`c.deposits`,
    `s.flows`, `s.trades`), contract/option/event pattern bindings, `let` bindings,
    `cycle(...)` via `generate_series`, and `max(...)` comprehensions.
  - `partition` declarations as `CREATE OR REPLACE VIEW` supporting arbitrary nested
    multi-way splits (`moreSplits`) and case-local `let` bindings.
-/

set_option autoImplicit false

namespace Prudence.SQL

open Prudence.DSL

private def sqlStr (s : String) : String :=
  "'" ++ s.replace "'" "''" ++ "'"

private def sqlIdent (s : String) : String :=
  let clean := s.replace "." "_" |>.replace "-" "_" |>.replace " " "_"
  let reserved := ["select", "from", "where", "group", "order", "case", "when", "then",
                   "else", "end", "table", "view", "limit", "offset", "in", "is", "not",
                   "and", "or", "as", "by", "on", "join", "union", "all", "distinct",
                   "check", "default", "primary", "key", "index", "value", "values",
                   "window", "row", "rows", "range", "current", "set", "class", "type",
                   "level", "date", "time", "timestamp", "interval", "user", "grant"]
  if reserved.contains clean.toLower then "\"" ++ clean ++ "\"" else clean

private def cellCol (item col : String) : String :=
  "c_" ++ (item.replace "." "_" |>.replace "-" "_") ++ "__" ++ col

private def stripAnn : Expr → Expr
  | .Annotated e _ => stripAnn e
  | e => e

private def binOpSql : BinOp → String
  | .Implies => "OR"
  | .Or      => "OR"
  | .And     => "AND"
  | .Eq      => "="
  | .Ne      => "<>"
  | .Lt      => "<"
  | .Le      => "<="
  | .Gt      => ">"
  | .Ge      => ">="
  | .In      => "IN"
  | .Add     => "+"
  | .Sub     => "-"
  | .Mul     => "*"
  | .Div     => "/"

-- ============================================================================
-- Form SQL Compiler (DuckDB)
-- ============================================================================

private partial def collectCellRefs (e : Expr) : List (String × String) :=
  match e with
  | .Cell i c _ => [(i, c)]
  | .List xs | .Tuple xs => xs.flatMap collectCellRefs
  | .Unary _ a => collectCellRefs a
  | .Binary _ a b => collectCellRefs a ++ collectCellRefs b
  | .Call f args => collectCellRefs f ++ args.flatMap (fun | .mk _ v => collectCellRefs v)
  | .Index t i => collectCellRefs t ++ collectCellRefs i
  | .Member t _ | .Is t _ => collectCellRefs t
  | .If c t o => collectCellRefs c ++ collectCellRefs t ++ collectCellRefs o
  | .IfLet _ v t o => collectCellRefs v ++ collectCellRefs t ++ collectCellRefs o
  | .Match s arms => collectCellRefs s ++ arms.flatMap (fun | .mk _ g b => (g.map collectCellRefs).getD [] ++ collectCellRefs b)
  | .Let _ v b => collectCellRefs v ++ collectCellRefs b
  | .Comprehension _ s flt b => collectCellRefs s ++ (flt.map collectCellRefs).getD [] ++ (b.map collectCellRefs).getD []
  | .Group s k flt fs => collectCellRefs s ++ collectCellRefs k ++ (flt.map collectCellRefs).getD [] ++ fs.flatMap (fun (_, v) => collectCellRefs v)
  | .Optimize _ flt _ obj cs => (flt.map collectCellRefs).getD [] ++ collectCellRefs obj ++ cs.flatMap collectCellRefs
  | .Forall _ _ b => collectCellRefs b
  | .Annotated x _ => collectCellRefs x
  | _ => []

private partial def formExprSql (e : Expr) : String :=
  match stripAnn e with
  | .Num v _ => s!"CAST({v} AS DECIMAL(38, 10))"
  | .Percent v _ => s!"(CAST({v} AS DECIMAL(38, 10)) / 100)"
  | .Str s _ => sqlStr s
  | .Ident n _ => sqlIdent n
  | .Cell i c _ => cellCol i c
  | .Unary .Not a => s!"(NOT ({formExprSql a}))"
  | .Unary .Neg a => s!"(-({formExprSql a}))"
  | .Binary .Implies a b => s!"((NOT ({formExprSql a})) OR ({formExprSql b}))"
  | .Binary .Div a b => s!"(({formExprSql a}) / NULLIF(({formExprSql b}), 0))"
  | .Binary op a b => s!"(({formExprSql a}) {binOpSql op} ({formExprSql b}))"
  | .If c t o => s!"(CASE WHEN {formExprSql c} THEN {formExprSql t} ELSE {formExprSql o} END)"
  | .Call (.Ident "min" _) args =>
    let xs := args.map (fun | .mk _ v => formExprSql v)
    s!"LEAST({String.intercalate ", " xs})"
  | .Call (.Ident "max" _) args =>
    let xs := args.map (fun | .mk _ v => formExprSql v)
    s!"GREATEST({String.intercalate ", " xs})"
  | .Call (.Ident "abs" _) [.mk _ v] => s!"ABS({formExprSql v})"
  | .Call (.Ident "sum" _) args =>
    let xs := args.map (fun | .mk _ v => formExprSql v)
    if xs.isEmpty then "CAST(0 AS DECIMAL(38, 10))"
    else "(" ++ String.intercalate " + " xs ++ ")"
  | .Call f args =>
    let xs := args.map (fun | .mk _ v => formExprSql v)
    s!"{formExprSql f}({String.intercalate ", " xs})"
  | .List xs => "(" ++ String.intercalate ", " (xs.map formExprSql) ++ ")"
  | _ => "NULL"

private def topoSortCells (cells : List ((String × String) × CellDef)) : List ((String × String) × CellDef) :=
  let keys := cells.map Prod.fst
  let rec go (fuel : Nat) (done : List (String × String)) (rem : List ((String × String) × CellDef))
      (acc : List ((String × String) × CellDef)) : List ((String × String) × CellDef) :=
    match fuel, rem with
    | 0, _ => acc.reverse ++ rem
    | _, [] => acc.reverse
    | n + 1, _ =>
      let ready := rem.filter fun (_, defn) =>
        match defn with
        | .Input _ => true
        | .Expr e =>
          let refs := collectCellRefs e
          refs.all fun r => !keys.contains r || done.contains r
      if ready.isEmpty then acc.reverse ++ rem
      else
        let readyKeys := ready.map Prod.fst
        let rem' := rem.filter fun (k, _) => !readyKeys.contains k
        go n (readyKeys ++ done) rem' (ready.reverse ++ acc)
  go (cells.length + 1) [] cells []

/-- Compile a `FormDecl` into a complete DuckDB SQL script (views + check query). -/
def compileForm (f : FormDecl) : String :=
  let allCells : List ((String × String) × CellDef) :=
    f.items.flatMap fun it => it.cells.map fun (col, defn) => ((it.id, col), defn)
  let inputs := allCells.filter fun (_, d) => match d with | .Input _ => true | .Expr _ => false
  let derived := topoSortCells (allCells.filter fun (_, d) => match d with | .Expr _ => true | .Input _ => false)
  let rec buildLayers (fuel : Nat) (avail : List (String × String))
      (rem : List ((String × String) × CellDef))
      (layers : List (List ((String × String) × CellDef))) : List (List ((String × String) × CellDef)) :=
    match fuel, rem with
    | 0, _ => if rem.isEmpty then layers.reverse else (rem :: layers).reverse
    | _, [] => layers.reverse
    | n + 1, _ =>
      let (now, later) := rem.partition fun (_, d) =>
        match d with
        | .Expr e => (collectCellRefs e).all fun r => avail.contains r
        | .Input _ => true
      if now.isEmpty then (rem :: layers).reverse
      else
        let newAvail := (now.map Prod.fst) ++ avail
        buildLayers n newAvail later (now :: layers)
  let layers := buildLayers (derived.length + 1) (inputs.map Prod.fst) derived []
  let viewName := sqlIdent f.name.toLower
  let inputCols :=
    if inputs.isEmpty then ["  CAST(0 AS DECIMAL(38, 10)) AS _dummy"]
    else inputs.map fun ((i, col), d) =>
      let dflt := match d with
        | .Input (some e) => formExprSql e
        | _ => "CAST(0 AS DECIMAL(38, 10))"
      s!"  COALESCE(MAX(CASE WHEN item_id = {sqlStr i} AND col = {sqlStr col} THEN val END), {dflt}) AS {cellCol i col}"
  let cte0 :=
    s!"  layer_0 AS (\n    SELECT\n{String.intercalate ",\n" inputCols}\n    FROM {viewName}_inputs\n  )"
  let layerCtes := layers.zipIdx.map fun (layer, idx) =>
    let prev := s!"layer_{idx}"
    let cur := s!"layer_{idx + 1}"
    let cols := layer.map fun ((i, col), d) =>
      match d with
      | .Expr e => s!"    {formExprSql e} AS {cellCol i col}"
      | .Input _ => s!"    CAST(0 AS DECIMAL(38, 10)) AS {cellCol i col}"
    s!"  {cur} AS (\n    SELECT\n      {prev}.*,\n{String.intercalate ",\n" cols}\n    FROM {prev}\n  )"
  let lastLayer := s!"layer_{layers.length}"
  let allCtes := String.intercalate ",\n" (cte0 :: layerCtes)
  let unpivotRows := allCells.map fun ((i, col), _) =>
    s!"  SELECT {sqlStr i} AS item_id, {sqlStr col} AS col, {cellCol i col} AS val FROM {lastLayer}"
  let checksSql :=
    if f.checks.isEmpty then ""
    else
      let checkRows := f.checks.map fun c =>
        let sev := if c.warn then "WARN" else "ERROR"
        s!"  SELECT {sqlStr c.id} AS check_id, {sqlStr sev} AS severity, {sqlStr c.description} AS description, ({formExprSql c.cond}) AS passed FROM {viewName}_wide"
      s!"\n-- Form validation checks (rows with passed = FALSE fail)\nCREATE OR REPLACE VIEW {viewName}_checks AS\n{String.intercalate "\nUNION ALL\n" checkRows};\n"
  s!"-- GENERATED by Prudence Lean 4 DSL (`lake exe prudence-export`)
-- Target: DuckDB SQL
-- Form: {f.code} — {f.title}
-- Items: {f.items.length}, Inputs: {inputs.length}, Derived: {derived.length}

CREATE TABLE IF NOT EXISTS {viewName}_inputs (
  item_id VARCHAR NOT NULL,
  col     VARCHAR NOT NULL,
  val     DECIMAL(38, 10) NOT NULL
);

CREATE OR REPLACE VIEW {viewName}_wide AS
WITH
{allCtes}
SELECT * FROM {lastLayer};

CREATE OR REPLACE VIEW {viewName}_cells AS
WITH {lastLayer} AS (SELECT * FROM {viewName}_wide)
{String.intercalate "\nUNION ALL\n" unpivotRows};
{checksSql}"

-- ============================================================================
-- Module SQL Compiler (DuckDB — fully lowered)
-- ============================================================================

private structure SqlCtx where
  modName        : String
  enums          : List (String × List String)
  params         : List String
  views          : List String
  allocs         : List String
  tableFns       : List String
  dataFields     : List (String × List (String × Ty))
  contractFields : List ((String × String) × List String)
  eventFields    : List (String × List String)
  fnParamTys     : List (String × List (String × Ty))

private structure Env where
  bindings : List (String × String) := []
  varTys   : List (String × String) := []

private def Env.bind (env : Env) (k v : String) : Env :=
  { env with bindings := (k, v) :: env.bindings }

private def Env.bindTy (env : Env) (k t : String) : Env :=
  { env with varTys := (k, t) :: env.varTys }

private def Env.lookup (env : Env) (k : String) : Option String :=
  (env.bindings.find? fun (n, _) => n == k).map Prod.snd

private def Env.lookupTy (env : Env) (k : String) : Option String :=
  (env.varTys.find? fun (n, _) => n == k).map Prod.snd

private def SqlCtx.isEnum (ctx : SqlCtx) (n : String) : Bool :=
  ctx.enums.any fun (_, ctors) => ctors.contains n

private def SqlCtx.isData (ctx : SqlCtx) (n : String) : Bool :=
  ctx.dataFields.any fun (t, _) => t == n

private def SqlCtx.fieldTy (ctx : SqlCtx) (dataName fieldName : String) : Option Ty :=
  (ctx.dataFields.find? fun (t, _) => t == dataName).bind fun (_, fs) =>
    (fs.find? fun (f, _) => f == fieldName).map Prod.snd

private def SqlCtx.backRefCol (ctx : SqlCtx) (elemData ownerData : String) : Option String :=
  (ctx.dataFields.find? fun (t, _) => t == elemData).bind fun (_, fs) =>
    (fs.find? fun (_, ty) => match ty with | .Named n _ _ _ => n == ownerData | _ => false).map Prod.fst

private def SqlCtx.ctorFields (ctx : SqlCtx) (dataName ctor : String) : List String :=
  (ctx.contractFields.find? fun ((d, c), _) => d == dataName && c == ctor).map Prod.snd |>.getD []

private def SqlCtx.eventFieldNames (ctx : SqlCtx) (evName : String) : List String :=
  (ctx.eventFields.find? fun (e, _) => e == evName).map Prod.snd |>.getD []

private def sqlType : Ty → String
  | .Named "Int" _ _ _ => "BIGINT"
  | .Named "Bool" _ _ _ => "BOOLEAN"
  | .Named "String" _ _ _ => "VARCHAR"
  | .Named "Date" _ _ _ => "DATE"
  | .Named "Money" _ _ _ => "DECIMAL(38, 10)"
  | .Named "Rate" _ _ _ => "DECIMAL(38, 10)"
  | .Named "Uncertain" [inner] _ _ => sqlType inner
  | .Named "Option" [inner] _ _ => sqlType inner
  | .Named "List" [inner] _ _ => s!"{sqlType inner}[]"
  | .Named _ _ _ _ => "VARCHAR"
  | .Union _ => "VARCHAR"

private partial def inferDataTy (ctx : SqlCtx) (env : Env) (e : Expr) : Option String :=
  match stripAnn e with
  | .Ident n _ => env.lookupTy n
  | .Member t f =>
    (inferDataTy ctx env t).bind fun dt =>
      match ctx.fieldTy dt f with
      | some (.Named n _ _ _) => if ctx.isData n then some n else none
      | _ => none
  | _ => none

private partial def isFkExpr (ctx : SqlCtx) (env : Env) (e : Expr) : Option String :=
  match stripAnn e with
  | .Member base fk =>
    (inferDataTy ctx env base).bind fun dt =>
      match ctx.fieldTy dt fk with
      | some (.Named tn _ _ _) => if ctx.isData tn then some tn else none
      | _ => none
  | .Ident n _ =>
    -- Inside `group`, row fields bound to `_g.col` are VARCHAR FKs, while `_j_<col>` are joined STRUCTs.
    match env.lookup n, env.lookupTy n with
    | some bound, some tn =>
      if ctx.isData tn && !bound.startsWith "_j_" then some tn else none
    | _, _ => none
  | _ => none

private partial def recordIdSql (ctx : SqlCtx) (env : Env) (e : Expr) (rendered : String) : String :=
  match isFkExpr ctx env e with
  | some _ => rendered
  | none =>
    match inferDataTy ctx env e with
    | some _ => s!"({rendered}).id"
    | none => rendered

private def paramMacroName (p : String) : String :=
  "_p_" ++ p

private def buildCtx (m : Module) : SqlCtx :=
  let enums := m.decls.filterMap fun d =>
    match d.kind with
    | .TypeAlias (.Union alts) =>
      let cs := alts.filterMap fun | .Named n _ _ _ => some n | _ => none
      some (d.name, cs)
    | _ => none
  let params := m.decls.filterMap fun d =>
    match d.kind with | .Param _ _ => some d.name | _ => none
  let views := m.decls.filterMap fun d =>
    match d.kind with | .View _ => some d.name | _ => none
  let allocs := m.decls.filterMap fun d =>
    match d.kind with | .Alloc _ _ => some d.name | _ => none
  let tableFns := m.decls.filterMap fun d =>
    match d.kind with
    | .Fn _ (.Named "List" _ _ _) (some _) => some d.name
    | _ => none
  let dataFields := m.decls.filterMap fun d =>
    match d.kind with
    | .Data _ fs => some (d.name, fs.map fun f => (f.name, f.ty))
    | _ => none
  let contractFields := m.decls.flatMap fun d =>
    match d.kind with
    | .Data (some (.Named "Contract" [g] _ _)) _ =>
      let alts := match g with | .Union xs => xs | x => [x]
      alts.filterMap fun
        | .Named ctor _ ps _ =>
          let fs := ps.filterMap fun | .Field n _ => some n | .Expr _ => none
          some ((d.name, ctor), fs)
        | _ => none
    | _ => []
  let eventFields := m.decls.filterMap fun d =>
    match d.kind with
    | .Event fs => some (d.name, fs.map Field.name)
    | _ => none
  let fnParamTys := m.decls.filterMap fun d =>
    match d.kind with
    | .Fn ps _ _ => some (d.name, ps.map fun p => (p.name, p.ty))
    | _ => none
  { modName := m.name, enums, params, views, allocs, tableFns, dataFields, contractFields, eventFields, fnParamTys }

mutual

  /-- Render a `Comprehension` source into `(fromClause, extraWhereOpt, elemDataTyOpt)`. -/
  private partial def compSourceSql (ctx : SqlCtx) (env : Env) (var : String) (src : Expr)
      : String × Option String × Option String :=
    match stripAnn src with
    -- One-to-many list field on a record, e.g. `c.deposits`, `s.flows`, `s.trades`
    | .Member target field =>
      match inferDataTy ctx env target with
      | some ownerData =>
        match ctx.fieldTy ownerData field with
        | some (.Named "List" [.Named elemData _ _ _] _ _) =>
          if ctx.isData elemData then
            let targetSql := recordIdSql ctx env target (exprSql ctx env target)
            let fk := (ctx.backRefCol elemData ownerData).getD ownerData.toLower
            (s!"{sqlIdent elemData.toLower} AS {sqlIdent var}",
             some s!"{sqlIdent var}.{sqlIdent fk} = {targetSql}",
             some elemData)
          else
            (s!"UNNEST({exprSql ctx env src}) AS _t({sqlIdent var})", none, none)
        | _ =>
          (s!"UNNEST({exprSql ctx env src}) AS _t({sqlIdent var})", none, none)
      | none =>
        (s!"UNNEST({exprSql ctx env src}) AS _t({sqlIdent var})", none, none)
    -- `cycle(start_date, step_months, end_date)` -> DuckDB `generate_series`
    | .Call (.Ident "cycle" _) [.mk _ startE, .mk _ stepE, .mk _ endE] =>
      let st := exprSql ctx env startE
      let sp := exprSql ctx env stepE
      let en := exprSql ctx env endE
      let gs := s!"generate_series(0, CAST(GREATEST(0, DATEDIFF('month', {st}, {en}) / NULLIF({sp}, 0) + 1) AS BIGINT)) AS _gs({sqlIdent var})"
      let cond := s!"({st} + INTERVAL ({sqlIdent var} * {sp}) MONTH) <= {en}"
      (gs, some cond, none)
    -- Bitemporal `events(as_of: ..., known_at: ...)`
    | .Call (.Ident "events" _) args =>
      let asOf := (args.find? fun | .mk (some "as_of") _ => true | _ => false).map (fun | .mk _ v => exprSql ctx env v) |>.getD "_p_reporting_date()"
      let knownAt := (args.find? fun | .mk (some "known_at") _ => true | _ => false).map (fun | .mk _ v => exprSql ctx env v) |>.getD "_p_known_at()"
      (s!"event_log AS {sqlIdent var}",
       some s!"{sqlIdent var}.valid <= {asOf} AND {sqlIdent var}.recorded <= {knownAt}",
       none)
    -- Table macro call, e.g. `realised(c)`, `window_ends(c)`
    | .Call (.Ident fnName _) args =>
      if ctx.tableFns.contains fnName then
        let xs := args.map fun | .mk _ v => exprSql ctx env v
        if fnName == "window_ends" then
          (s!"{sqlIdent fnName}({String.intercalate ", " xs}) AS _t({sqlIdent var})", none, none)
        else
          (s!"{sqlIdent fnName}({String.intercalate ", " xs}) AS {sqlIdent var}", none, none)
      else
        (s!"({exprSql ctx env src}) AS _t({sqlIdent var})", none, none)
    -- Data table directly, e.g. `NettingSet`
    | .Ident n _ =>
      if ctx.isData n then
        (s!"{sqlIdent n.toLower} AS {sqlIdent var}", none, some n)
      else
        (s!"{sqlIdent n.toLower} AS {sqlIdent var}", none, none)
    | _ =>
      (s!"({exprSql ctx env src}) AS _t({sqlIdent var})", none, none)

  private partial def exprSql (ctx : SqlCtx) (env : Env) (e : Expr) : String :=
    match stripAnn e with
    | .Num v _ => s!"CAST({v} AS DECIMAL(38, 10))"
    | .Percent v _ => s!"(CAST({v} AS DECIMAL(38, 10)) / 100)"
    | .Str s _ => sqlStr s
    | .Ident "true" _ => "TRUE"
    | .Ident "false" _ => "FALSE"
    | .Ident "none" _ => "NULL"
    | .Ident n _ =>
      match env.lookup n with
      | some bound => bound
      | none =>
        if ctx.params.contains n then s!"{paramMacroName n}()"
        else if ctx.isEnum n then sqlStr n
        else sqlIdent n
    | .Cell i c _ => sqlStr s!"{i}:{c}"
    | .List xs => "[" ++ String.intercalate ", " (xs.map (exprSql ctx env)) ++ "]"
    | .Tuple xs => "ROW(" ++ String.intercalate ", " (xs.map (exprSql ctx env)) ++ ")"
    | .Unary .Not a => s!"(NOT ({exprSql ctx env a}))"
    | .Unary .Neg a => s!"(-({exprSql ctx env a}))"
    | .Binary .Implies a b => s!"((NOT ({exprSql ctx env a})) OR ({exprSql ctx env b}))"
    | .Binary .In a (.List xs) =>
      s!"(({exprSql ctx env a}) IN ({String.intercalate ", " (xs.map (exprSql ctx env))}))"
    | .Binary .In a b =>
      s!"(({exprSql ctx env a}) IN (SELECT * FROM {exprSql ctx env b}))"
    | .Binary .Eq a (.Ident "none" _) => s!"(({exprSql ctx env a}) IS NULL)"
    | .Binary .Ne a (.Ident "none" _) => s!"(({exprSql ctx env a}) IS NOT NULL)"
    | .Binary .Eq (.Ident "none" _) b => s!"(({exprSql ctx env b}) IS NULL)"
    | .Binary .Ne (.Ident "none" _) b => s!"(({exprSql ctx env b}) IS NOT NULL)"
    | .Binary .Eq a b =>
      let sa := recordIdSql ctx env a (exprSql ctx env a)
      let sb := recordIdSql ctx env b (exprSql ctx env b)
      s!"(({sa}) = ({sb}))"
    | .Binary .Ne a b =>
      let sa := recordIdSql ctx env a (exprSql ctx env a)
      let sb := recordIdSql ctx env b (exprSql ctx env b)
      s!"(({sa}) <> ({sb}))"
    | .Binary .Div a b => s!"(({exprSql ctx env a}) / NULLIF(({exprSql ctx env b}), 0))"
    | .Binary op a b => s!"(({exprSql ctx env a}) {binOpSql op} ({exprSql ctx env b}))"
    | .Member t "days" => s!"INTERVAL (CAST({exprSql ctx env t} AS BIGINT)) DAY"
    | .Member t "AUD" => exprSql ctx env t
    -- View lookup field: `SmeGroups[g].total`
    | .Member (.Index (.Ident vName _) keyE) fld =>
      if ctx.views.contains vName then
        s!"COALESCE((SELECT _v.{sqlIdent fld} FROM {sqlIdent vName.toLower} AS _v WHERE _v.grp_key = ({exprSql ctx env keyE})), 0)"
      else
        s!"({exprSql ctx env (.Index (.Ident vName {}) keyE)}).{sqlIdent fld}"
    -- FK member access (handles both `d.holder.kind` and `holder.kind` inside `group`)
    | .Member target fld =>
      match isFkExpr ctx env target with
      | some targetData =>
        s!"(SELECT _fk.{sqlIdent fld} FROM {sqlIdent targetData.toLower} AS _fk WHERE _fk.id = ({exprSql ctx env target}))"
      | none =>
        s!"({exprSql ctx env target}).{sqlIdent fld}"
    -- Allocation lookup: `covered[d]`, `insured[d]`
    | .Index (.Ident aName _) keyE =>
      if ctx.allocs.contains aName then
        let keySql :=
          match inferDataTy ctx env keyE with
          | some _ => s!"({exprSql ctx env keyE}).id"
          | none => exprSql ctx env keyE
        s!"COALESCE((SELECT _a.amount FROM alloc_{sqlIdent aName.toLower} AS _a WHERE _a.record_id = {keySql} AND _a.mode = _p_mode()), 0)"
      else if ctx.views.contains aName then
        s!"(SELECT _v FROM {sqlIdent aName.toLower} AS _v WHERE _v.grp_key = ({exprSql ctx env keyE}) LIMIT 1)"
      else
        s!"{exprSql ctx env (.Ident aName {})}[{exprSql ctx env keyE}]"
    | .Index t i => s!"{exprSql ctx env t}[{exprSql ctx env i}]"
    | .Is t c => s!"(({exprSql ctx env t}).event_kind = {sqlStr c})"
    | .If c t o => s!"(CASE WHEN {exprSql ctx env c} THEN {exprSql ctx env t} ELSE {exprSql ctx env o} END)"
    -- `if let Some(m) = val then ... else ...`
    | .IfLet (.Ctor "Some" [.Bind x]) v t o =>
      let vSql := exprSql ctx env v
      let env' := env.bind x vSql
      s!"(CASE WHEN ({vSql}) IS NOT NULL THEN {exprSql ctx env' t} ELSE {exprSql ctx env o} END)"
    -- `if let EventCtor(p1, p2) = val then ... else ...`
    | .IfLet (.Ctor ctor pats) v t o =>
      let vSql := exprSql ctx env v
      let fnames := ctx.eventFieldNames ctor
      let env' := (pats.zip fnames).foldl (init := env) fun acc (p, fname) =>
        match p with
        | .Bind x => acc.bind x s!"({vSql}).{sqlIdent fname}"
        | _ => acc
      s!"(CASE WHEN ({vSql}) IS NOT NULL THEN {exprSql ctx env' t} ELSE {exprSql ctx env o} END)"
    | .IfLet (.Bind x) v t o =>
      let vSql := exprSql ctx env v
      let env' := env.bind x vSql
      s!"(CASE WHEN ({vSql}) IS NOT NULL THEN {exprSql ctx env' t} ELSE {exprSql ctx env o} END)"
    | .IfLet _ v t o =>
      s!"(CASE WHEN ({exprSql ctx env v}) IS NOT NULL THEN {exprSql ctx env t} ELSE {exprSql ctx env o} END)"
    -- Pure inline `let` binding
    | .Let (.Bind x) v b =>
      let vSql := "(" ++ exprSql ctx env v ++ ")"
      exprSql ctx (env.bind x vSql) b
    | .Let _ _ b => exprSql ctx env b
    -- `match` (handles `rec.contract` with variant field bindings + enums + tuples)
    | .Match scr arms =>
      let contractTarget : Option (String × String) :=
        match stripAnn scr with
        | .Member rec "contract" => (inferDataTy ctx env rec).map fun dt => (exprSql ctx env rec, dt)
        | _ => none
      let sSql := match contractTarget with
        | some (recSql, _) => s!"({recSql}).contract_kind"
        | none => exprSql ctx env scr
      let branches := arms.map fun a =>
        match a with
        | .mk pat guard body =>
          let env' := match contractTarget, pat with
            | some (recSql, dt), .Ctor ctor pats =>
              let fnames := ctx.ctorFields dt ctor
              (pats.zip fnames).foldl (init := env) fun acc (p, fname) =>
                match p with
                | .Bind x => acc.bind x s!"({recSql}).{sqlIdent fname}"
                | _ => acc
            | _, _ => env
          let patCond := match pat with
            | .Wild | .Bind _ => "TRUE"
            | .Ctor c _ => s!"({sSql} = {sqlStr c})"
            | .Tuple ps =>
              let parts := ps.zipIdx.filterMap fun (p, i) =>
                match p with
                | .Wild | .Bind _ => none
                | .Ctor c _ => some s!"(({sSql})[{i + 1}] = {sqlStr c})"
                | _ => none
              if parts.isEmpty then "TRUE" else String.intercalate " AND " parts
          let fullCond := match guard with
            | none => patCond
            | some g => if patCond == "TRUE" then exprSql ctx env' g else s!"({patCond}) AND ({exprSql ctx env' g})"
          s!"WHEN {fullCond} THEN {exprSql ctx env' body}"
      s!"(CASE {String.intercalate " " branches} END)"
    -- `conservative(x, worst: w, best: b)`
    | .Call (.Ident "conservative" _) args =>
      let val := match args.head? with | some (.mk _ v) => exprSql ctx env v | none => "NULL"
      let worst := (args.find? fun | .mk (some "worst") _ => true | _ => false).map (fun | .mk _ v => exprSql ctx env v) |>.getD "NULL"
      let best := (args.find? fun | .mk (some "best") _ => true | _ => false).map (fun | .mk _ v => exprSql ctx env v)
        |>.getD (if worst == "TRUE" then "FALSE" else if worst == "FALSE" then "TRUE" else worst)
      s!"COALESCE({val}, CASE WHEN _p_mode() = 'Optimistic' THEN {best} ELSE {worst} END)"
    -- Aggregates over comprehensions
    | .Call (.Ident "sum" _) [.mk _ (.Comprehension var src flt body)] =>
      let (fromSql, extraWhere, elemTy) := compSourceSql ctx env var src
      let env' := match elemTy with | some dt => env.bindTy var dt | none => env
      let bodySql := (body.map (exprSql ctx env')).getD (sqlIdent var)
      let conds := [extraWhere, flt.map (exprSql ctx env')].filterMap id
      let whereSql := if conds.isEmpty then "" else " WHERE " ++ String.intercalate " AND " conds
      s!"COALESCE((SELECT SUM({bodySql}) FROM {fromSql}{whereSql}), 0)"
    | .Call (.Ident "max" _) [.mk _ (.Comprehension var src flt body)] =>
      let (fromSql, extraWhere, elemTy) := compSourceSql ctx env var src
      let env' := match elemTy with | some dt => env.bindTy var dt | none => env
      let bodySql := (body.map (exprSql ctx env')).getD (sqlIdent var)
      let conds := [extraWhere, flt.map (exprSql ctx env')].filterMap id
      let whereSql := if conds.isEmpty then "" else " WHERE " ++ String.intercalate " AND " conds
      s!"COALESCE((SELECT MAX({bodySql}) FROM {fromSql}{whereSql}), 0)"
    | .Call (.Ident "min" _) [.mk _ (.Comprehension var src flt body)] =>
      let (fromSql, extraWhere, elemTy) := compSourceSql ctx env var src
      let env' := match elemTy with | some dt => env.bindTy var dt | none => env
      let bodySql := (body.map (exprSql ctx env')).getD (sqlIdent var)
      let conds := [extraWhere, flt.map (exprSql ctx env')].filterMap id
      let whereSql := if conds.isEmpty then "" else " WHERE " ++ String.intercalate " AND " conds
      s!"COALESCE((SELECT MIN({bodySql}) FROM {fromSql}{whereSql}), 0)"
    | .Call (.Ident "all" _) [.mk _ (.Comprehension var src flt body)] =>
      let (fromSql, extraWhere, elemTy) := compSourceSql ctx env var src
      let env' := match elemTy with | some dt => env.bindTy var dt | none => env
      let bodySql := (body.map (exprSql ctx env')).getD "TRUE"
      let conds := [extraWhere, flt.map (exprSql ctx env')].filterMap id
      let whereSql := if conds.isEmpty then "" else " WHERE " ++ String.intercalate " AND " conds
      s!"COALESCE((SELECT BOOL_AND({bodySql}) FROM {fromSql}{whereSql}), TRUE)"
    | .Call (.Ident "any" _) [.mk _ (.Comprehension var src flt body)] =>
      let (fromSql, extraWhere, elemTy) := compSourceSql ctx env var src
      let env' := match elemTy with | some dt => env.bindTy var dt | none => env
      let bodySql := (body.map (exprSql ctx env')).getD "TRUE"
      let conds := [extraWhere, flt.map (exprSql ctx env')].filterMap id
      let whereSql := if conds.isEmpty then "" else " WHERE " ++ String.intercalate " AND " conds
      s!"COALESCE((SELECT BOOL_OR({bodySql}) FROM {fromSql}{whereSql}), FALSE)"
    | .Call (.Ident "distinct" _) [.mk _ (.Comprehension var src flt body)] =>
      let (fromSql, extraWhere, elemTy) := compSourceSql ctx env var src
      let env' := match elemTy with | some dt => env.bindTy var dt | none => env
      let bodySql := (body.map (exprSql ctx env')).getD (sqlIdent var)
      let conds := [extraWhere, flt.map (exprSql ctx env')].filterMap id
      let whereSql := if conds.isEmpty then "" else " WHERE " ++ String.intercalate " AND " conds
      s!"(SELECT DISTINCT {bodySql} FROM {fromSql}{whereSql})"
    | .Call (.Ident "latest" _) [.mk _ (.Comprehension var src flt body)] =>
      let (fromSql, extraWhere, elemTy) := compSourceSql ctx env var src
      let env' := match elemTy with | some dt => env.bindTy var dt | none => env
      let bodySql := (body.map (exprSql ctx env')).getD (sqlIdent var)
      let conds := [extraWhere, flt.map (exprSql ctx env')].filterMap id
      let whereSql := if conds.isEmpty then "" else " WHERE " ++ String.intercalate " AND " conds
      s!"(SELECT {bodySql} FROM {fromSql}{whereSql} ORDER BY {sqlIdent var}.valid DESC, {sqlIdent var}.recorded DESC LIMIT 1)"
    -- Scalar built-ins
    | .Call (.Ident "min" _) args =>
      let xs := args.map (fun | .mk _ v => exprSql ctx env v)
      s!"LEAST({String.intercalate ", " xs})"
    | .Call (.Ident "max" _) args =>
      let xs := args.map (fun | .mk _ v => exprSql ctx env v)
      s!"GREATEST({String.intercalate ", " xs})"
    | .Call (.Ident "abs" _) [.mk _ v] => s!"ABS({exprSql ctx env v})"
    | .Call (.Ident "days" _) [.mk _ a, .mk _ b] =>
      s!"DATEDIFF('day', {exprSql ctx env a}, {exprSql ctx env b})"
    | .Call (.Ident "add_months" _) [.mk _ d, .mk _ m] =>
      s!"({exprSql ctx env d} + INTERVAL (CAST({exprSql ctx env m} AS BIGINT)) MONTH)"
    | .Call (.Ident "pow" _) [.mk _ a, .mk _ b] =>
      s!"POWER(CAST({exprSql ctx env a} AS DOUBLE), CAST({exprSql ctx env b} AS DOUBLE))"
    | .Call (.Ident "item" _) [.mk _ p, .mk _ s] =>
      s!"({exprSql ctx env p} || '.' || CAST(CAST({exprSql ctx env s} AS BIGINT) AS VARCHAR))"
    | .Call (.Ident "solo" _) [.mk _ c] =>
      let cid := recordIdSql ctx env c (exprSql ctx env c)
      s!"('solo:' || {cid})"
    -- User-defined function calls (auto-resolve FK -> STRUCT if param expects a data record)
    | .Call (.Ident fnName _) args =>
      let expectedTys := (ctx.fnParamTys.find? fun (n, _) => n == fnName).map Prod.snd |>.getD []
      let renderedArgs := args.zipIdx.map fun (a, i) =>
        match a with
        | .mk _ v =>
          let wantData := match expectedTys[i]? with
            | some (_, .Named t _ _ _) => if ctx.isData t then some t else none
            | _ => none
          match wantData, stripAnn v with
          | some targetData, .Member base fk =>
            -- `class(d.holder)` -> fetch full `customer` STRUCT from FK `d.holder`
            s!"(SELECT _fk FROM {sqlIdent targetData.toLower} AS _fk WHERE _fk.id = ({exprSql ctx env base}).{sqlIdent fk})"
          | _, _ => exprSql ctx env v
      s!"{sqlIdent fnName}({String.intercalate ", " renderedArgs})"
    | .Call f args =>
      let xs := args.map (fun | .mk _ v => exprSql ctx env v)
      s!"{exprSql ctx env f}({String.intercalate ", " xs})"
    | .Group src key flt fields =>
      let srcName := match stripAnn src with | .Ident n _ => n | _ => "records"
      let rowAlias := "_g"
      let srcFields := (ctx.dataFields.find? fun (t, _) => t == srcName).map Prod.snd |>.getD []
      -- Emit explicit LEFT JOINs for FK data fields (e.g. `holder` -> `LEFT JOIN customer AS _j_holder`)
      let fkJoins := srcFields.filterMap fun (fname, fty) =>
        match fty with
        | .Named tn _ _ _ =>
          if ctx.isData tn then
            some (fname, tn, s!"_j_{sqlIdent fname}", s!"LEFT JOIN {sqlIdent tn.toLower} AS _j_{sqlIdent fname} ON _j_{sqlIdent fname}.id = {rowAlias}.{sqlIdent fname}")
          else none
        | _ => none
      let env0 := srcFields.foldl (init := (Env.mk [] [(rowAlias, srcName)])) fun acc (fname, fty) =>
        match fkJoins.find? fun (f, _, _, _) => f == fname with
        | some (_, tn, jAlias, _) => acc.bind fname jAlias |>.bindTy fname tn
        | none =>
          let acc' := acc.bind fname s!"{rowAlias}.{sqlIdent fname}"
          match fty with
          | .Named tn _ _ _ => if ctx.isData tn then acc'.bindTy fname tn else acc'
          | _ => acc'
      let k := exprSql ctx env0 key
      let whereSql := match flt with | some f => s!"\nWHERE {exprSql ctx env0 f}" | none => ""
      let aggs := fields.map fun (n, ex) => s!"  {exprSql ctx env0 ex} AS {sqlIdent n}"
      let joinSql := if fkJoins.isEmpty then "" else "\n" ++ String.intercalate "\n" (fkJoins.map fun (_, _, _, j) => j)
      s!"SELECT\n  {k} AS grp_key,\n{String.intercalate ",\n" aggs}\nFROM {sqlIdent srcName.toLower} AS {rowAlias}{joinSql}{whereSql}\nGROUP BY {k}"
    | _ => "NULL"

end

/-- Render a list-returning function body as a DuckDB `AS TABLE` query (handling list `+` as `UNION`). -/
private partial def tableQuerySql (ctx : SqlCtx) (env : Env) (e : Expr) : String :=
  match stripAnn e with
  | .Binary .Add a b =>
    s!"({tableQuerySql ctx env a})\nUNION\n({tableQuerySql ctx env b})"
  | .List xs =>
    let rows := xs.map fun x => s!"SELECT {exprSql ctx env x} AS val"
    String.intercalate "\nUNION\n" rows
  | .Call (.Ident "distinct" _) [.mk _ (.Comprehension var src flt body)] =>
    let (fromSql, extraWhere, elemTy) := compSourceSql ctx env var src
    let env' := match elemTy with | some dt => env.bindTy var dt | none => env
    let bodySql := match body with
      | some (.Ident v _) => if v == var then s!"{sqlIdent var}.*" else s!"{exprSql ctx env' (.Ident v {})} AS val"
      | some b => s!"{exprSql ctx env' b} AS val"
      | none => s!"{sqlIdent var}.*"
    let conds := [extraWhere, flt.map (exprSql ctx env')].filterMap id
    let whereSql := if conds.isEmpty then "" else " WHERE " ++ String.intercalate " AND " conds
    s!"SELECT DISTINCT {bodySql} FROM {fromSql}{whereSql}"
  | _ => s!"SELECT {exprSql ctx env e} AS val"

private def isNullableTy : Ty → Bool
  | .Named "Uncertain" _ _ _ => true
  | .Named "Option" _ _ _ => true
  | _ => false

private def colConstraints (ctx : SqlCtx) (fname : String) (ty : Ty) : String :=
  let nullable := isNullableTy ty
  let inner := match ty with
    | .Named "Uncertain" [t] _ _ => t
    | .Named "Option" [t] _ _ => t
    | t => t
  let notNull := if nullable then "" else " NOT NULL"
  let checkEnum := match inner with
    | .Named tn _ _ _ =>
      match ctx.enums.find? fun (e, _) => e == tn with
      | some (_, ctors) =>
        let vals := String.intercalate ", " (ctors.map sqlStr)
        if nullable then s!" CHECK ({sqlIdent fname} IS NULL OR {sqlIdent fname} IN ({vals}))"
        else s!" CHECK ({sqlIdent fname} IN ({vals}))"
      | none =>
        if ctx.isData tn then s!" REFERENCES {sqlIdent tn.toLower}(id)"
        else ""
    | _ => ""
  let nonNegFields := ["balance", "market_value", "amount", "cash", "collateral_value", "undrawn",
                       "value_after_rba_margin", "operational_balance", "minimum_payment",
                       "payment", "principal_per_period", "notice_days"]
  let checkNum :=
    if fname == "cycle_months" then s!" CHECK ({sqlIdent fname} > 0)"
    else if nonNegFields.contains fname then
      if nullable then s!" CHECK ({sqlIdent fname} IS NULL OR {sqlIdent fname} >= 0)"
      else s!" CHECK ({sqlIdent fname} >= 0)"
    else ""
  notNull ++ checkEnum ++ checkNum

private def contractCols (ctx : SqlCtx) (base : Option Ty) : List String :=
  match base with
  | some (.Named "Contract" [g] _ _) =>
    let alts := match g with | .Union xs => xs | x => [x]
    let ctors := alts.filterMap fun | .Named c _ _ _ => some c | _ => none
    let ckCheck :=
      if ctors.isEmpty then ""
      else s!" CHECK (contract_kind IN ({String.intercalate ", " (ctors.map sqlStr)}))"
    let variantFields := alts.flatMap fun
      | .Named _ _ ps _ => ps.filterMap fun
        | .Field n t =>
          let inner := match t with
            | .Named "Uncertain" [x] _ _ => x
            | .Named "Option" [x] _ _ => x
            | x => x
          let enumCheck := match inner with
            | .Named tn _ _ _ =>
              match ctx.enums.find? (fun (p : String × List String) => p.1 == tn) with
              | some (_, cs) => s!" CHECK ({sqlIdent n} IS NULL OR {sqlIdent n} IN ({String.intercalate ", " (cs.map sqlStr)}))"
              | none => ""
            | _ => ""
          some s!"  {sqlIdent n} {sqlType t}{enumCheck}"
        | .Expr _ => none
      | _ => []
    (s!"  contract_kind VARCHAR NOT NULL{ckCheck}") :: variantFields
  | _ => []

/-- Expand nested `partition` splits (`[(v1, e1), (v2, e2), ...]`) into `(flags, amountExprSql)` branches. -/
private def expandSplits (ctx : SqlCtx) (baseEnv : Env) (wholeSql : String)
    (splits : List (String × Expr)) : List (List (String × Bool) × String) :=
  splits.foldl (init := [([], wholeSql)]) fun portions (sVar, sExpr) =>
    portions.flatMap fun (flags, remAmt) =>
      let env' := flags.foldl (init := baseEnv) fun acc (k, b) =>
        acc.bind k (if b then "TRUE" else "FALSE")
      let partSql := exprSql ctx env' sExpr
      let restSql := s!"(({remAmt}) - ({partSql}))"
      [(flags ++ [(sVar, true)], partSql), (flags ++ [(sVar, false)], restSql)]

/-- Compile a `Module` into directly executable DuckDB SQL. -/
def compileModule (m : Module) : String :=
  let ctx := buildCtx m
  let modLower := sqlIdent m.name.toLower
  -- 1. Params table + zero-arg parameter & helper macros
  let paramLines := m.decls.filterMap fun d =>
    match d.kind with
    | .Param ty dflt =>
      let dStr := match dflt with | some e => s!" DEFAULT {exprSql ctx {} e}" | none => ""
      some s!"  {sqlIdent d.name} {sqlType ty}{dStr}"
    | _ => none
  let paramsDdl :=
    s!"CREATE TABLE IF NOT EXISTS {modLower}_params (\n  mode VARCHAR DEFAULT 'Conservative'" ++
    (if paramLines.isEmpty then "" else ",\n" ++ String.intercalate ",\n" paramLines) ++
    "\n);"
  let paramMacros :=
    [s!"CREATE OR REPLACE MACRO _p_mode() AS ((SELECT mode FROM {modLower}_params LIMIT 1));"] ++
    ctx.params.map fun p =>
      s!"CREATE OR REPLACE MACRO {paramMacroName p}() AS ((SELECT {sqlIdent p} FROM {modLower}_params LIMIT 1));"
  let builtinMacros :=
    if ctx.params.contains "reporting_date" then
      [s!"CREATE OR REPLACE MACRO bucket(d) AS (CASE WHEN d < (_p_reporting_date() + INTERVAL 6 MONTH) THEN 'b0' WHEN d < (_p_reporting_date() + INTERVAL 12 MONTH) THEN 'b1' ELSE 'b2' END);"]
    else []
  -- 2. Data & event tables (plus unified `event_log` view if events exist)
  let dataDdls := m.decls.filterMap fun d =>
    match d.kind with
    | .Data base fields =>
      let cols := ("  id VARCHAR PRIMARY KEY") ::
        (fields.filterMap fun f =>
          match f.ty with
          | .Named "List" _ _ _ => none -- one-to-many modeled via child table FK
          | _ => some s!"  {sqlIdent f.name} {sqlType f.ty}{colConstraints ctx f.name f.ty}") ++
        contractCols ctx base
      some s!"CREATE TABLE IF NOT EXISTS {sqlIdent d.name.toLower} (\n{String.intercalate ",\n" cols}\n);"
    | .Event fields =>
      let cols :=
        ["  event_id VARCHAR PRIMARY KEY", "  valid DATE NOT NULL", "  recorded DATE NOT NULL"] ++
        (fields.map fun f => s!"  {sqlIdent f.name} {sqlType f.ty}{colConstraints ctx f.name f.ty}")
      some s!"CREATE TABLE IF NOT EXISTS {sqlIdent d.name.toLower}_events (\n{String.intercalate ",\n" cols}\n);"
    | _ => none
  let eventLogView :=
    let evs := m.decls.filterMap fun d =>
      match d.kind with | .Event fs => some (d.name, fs.map Field.name) | _ => none
    if evs.isEmpty then []
    else
      let allCols := evs.flatMap Prod.snd |>.eraseDups
      let selects := evs.map fun (evName, fs) =>
        let proj := allCols.map fun c =>
          if fs.contains c then sqlIdent c else s!"NULL AS {sqlIdent c}"
        s!"  SELECT {sqlStr evName} AS event_kind, event_id, valid, recorded, {String.intercalate ", " proj} FROM {sqlIdent evName.toLower}_events"
      [s!"CREATE OR REPLACE VIEW event_log AS\n{String.intercalate "\nUNION ALL\n" selects};"]
  -- 3. External LP allocation tables
  let allocDdls := m.decls.filterMap fun d =>
    match d.kind with
    | .Alloc _ (.Optimize per _ sense _ _) =>
      let senseStr := match sense with | .Maximize => "MAXIMIZE" | .Minimize => "MINIMIZE"
      some s!"-- LP allocation `{d.name}` ({senseStr} per {per}):\n-- Solved externally by `prudence-core` exact simplex with primal-dual certificates\n-- and loaded into this table before running the partition view.\nCREATE TABLE IF NOT EXISTS alloc_{sqlIdent d.name.toLower} (\n  record_id VARCHAR PRIMARY KEY,\n  mode      VARCHAR NOT NULL,\n  amount    DECIMAL(38, 10) NOT NULL\n);"
    | _ => none
  -- 4. Views
  let viewDdls := m.decls.filterMap fun d =>
    match d.kind with
    | .View (.Group src key flt fields) =>
      some s!"CREATE OR REPLACE VIEW {sqlIdent d.name.toLower} AS\n{exprSql ctx {} (.Group src key flt fields)};"
    | .View e =>
      some s!"CREATE OR REPLACE VIEW {sqlIdent d.name.toLower} AS\n{tableQuerySql ctx {} e};"
    | _ => none
  -- 5. Functions (scalar macros + table macros for List-returning fns)
  let fnDdls := m.decls.filterMap fun d =>
    match d.kind with
    | .Fn params ret (some body) =>
      let ps := params.map fun p => sqlIdent p.name
      let env0 := params.foldl (init := Env.mk [] []) fun acc p =>
        match p.ty with
        | .Named tn _ _ _ => if ctx.isData tn then acc.bindTy p.name tn else acc
        | _ => acc
      match ret with
      | .Named "List" _ _ _ =>
        some s!"CREATE OR REPLACE MACRO {sqlIdent d.name}({String.intercalate ", " ps}) AS TABLE (\n  {tableQuerySql ctx env0 body}\n);"
      | _ =>
        some s!"CREATE OR REPLACE MACRO {sqlIdent d.name}({String.intercalate ", " ps}) AS (\n  {exprSql ctx env0 body}\n);"
    | _ => none
  -- 6. Partition views (with full multi-way nested splits & case-local lets)
  let partDdls := m.decls.filterMap fun d =>
    match d.kind with
    | .Partition var src targets split splitVar splitBy moreSplits cases =>
      let srcData := match src with | .Named n _ _ _ => n | _ => "Record"
      let srcTable := sqlIdent srcData.toLower
      let srcFields := (ctx.dataFields.find? fun (t, _) => t == srcData).map Prod.snd |>.getD []
      let fkJoins := srcFields.filterMap fun (fname, fty) =>
        match fty with
        | .Named tn _ _ _ =>
          if ctx.isData tn then
            some s!"LEFT JOIN {sqlIdent tn.toLower} AS _j_{sqlIdent fname} ON _j_{sqlIdent fname}.id = {sqlIdent var}.{sqlIdent fname}"
          else none
        | _ => none
      let allocJoins := ctx.allocs.map fun a =>
        s!"LEFT JOIN alloc_{sqlIdent a.toLower} AS _j_alloc_{sqlIdent a.toLower} ON _j_alloc_{sqlIdent a.toLower}.record_id = {sqlIdent var}.id AND _j_alloc_{sqlIdent a.toLower}.mode = _p_mode()"
      let joinSql :=
        let js := fkJoins ++ allocJoins
        if js.isEmpty then "" else "\n" ++ String.intercalate "\n" js
      let baseEnv := Env.mk [] [(var, srcData)]
      let wholeSql := exprSql ctx baseEnv split
      let splits : List (String × Expr) :=
        if splitVar.isEmpty || splitVar == "_whole" then []
        else (splitVar, splitBy) :: moreSplits
      let branches :=
        if splits.isEmpty then [([], wholeSql)]
        else expandSplits ctx baseEnv wholeSql splits
      let outCols := (cases.head?.map fun c => c.outputs.map Prod.fst).getD []
      let selects := branches.map fun (flags, amtSql) =>
        let branchEnv := flags.foldl (init := baseEnv) fun acc (k, b) =>
          acc.bind k (if b then "TRUE" else "FALSE")
        let flagCols := flags.map fun (k, b) => s!"  {if b then "TRUE" else "FALSE"} AS {sqlIdent k}"
        let targetSelects := outCols.map fun outName =>
          let whenClauses := cases.map fun c =>
            let caseEnv := c.lets.foldl (init := branchEnv) fun acc (pat, valE) =>
              match pat with
              | .Bind x => acc.bind x ("(" ++ exprSql ctx acc valE ++ ")")
              | _ => acc
            let val := (c.outputs.find? fun (k, _) => k == outName).map (exprSql ctx caseEnv ∘ Prod.snd) |>.getD "NULL"
            s!"WHEN {exprSql ctx branchEnv c.cond} THEN {val}"
          s!"  CASE {String.intercalate " " whenClauses} END AS {sqlIdent outName}"
        let allCols :=
          [s!"  {sqlIdent var}.id AS record_id"] ++
          flagCols ++
          [s!"  {amtSql} AS amount"] ++
          targetSelects
        let filterClause :=
          if splits.isEmpty then ""
          else s!"\nWHERE ({amtSql}) > 0"
        s!"SELECT\n{String.intercalate ",\n" allCols}\nFROM {srcTable} AS {sqlIdent var}{joinSql}{filterClause}"
      some s!"-- Partition into {String.intercalate ", " targets}\nCREATE OR REPLACE VIEW {modLower}_partition AS\n{String.intercalate "\nUNION ALL\n" selects};"
    | _ => none
  let sections := [
    paramsDdl,
    String.intercalate "\n" (paramMacros ++ builtinMacros),
    String.intercalate "\n\n" (dataDdls ++ eventLogView),
    String.intercalate "\n\n" allocDdls,
    String.intercalate "\n\n" viewDdls,
    String.intercalate "\n\n" fnDdls,
    String.intercalate "\n\n" partDdls
  ]
  let body := String.intercalate "\n\n" (sections.filter fun s => !s.isEmpty)
  s!"-- GENERATED by Prudence Lean 4 DSL (`lake exe prudence-export`)
-- Target: DuckDB SQL
-- Module: {m.name}

{body}
"

end Prudence.SQL
