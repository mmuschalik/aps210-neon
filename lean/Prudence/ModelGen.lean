import Prudence.DSL

/-!
# Prudence Pure Lean 4 Spec-to-Proof-Model Generator (`Prudence.ModelGen`)

Compiles any `Prudence.DSL.Module` (and target `FormDecl` weight/factor tables)
directly into its executable Lean 4 proof model (`lean/Prudence/<Module>.lean`).

This keeps `lean/Prudence/Specs/` as the **single source of truth**:
`lake exe prudence-export ..` regenerates:
1. `specs/ir/**/*.json`  (Prudence IR for Rust/Z3/LP/Checker)
2. `sql/**/*.sql`        (DuckDB analytical SQL)
3. `lean/Prudence/*.lean` (Lean 4 proof models imported by `*Proofs.lean`)
-/

set_option autoImplicit false

namespace Prudence.ModelGen

open Prudence.DSL

inductive Measure where
  | Weight (rows : List (String × Int))
  | Factor (rows : List (String × Nat × Int))

private def reserved : List String := [
  "class", "match", "fun", "def", "let", "if", "then", "else", "do", "end",
  "where", "structure", "prefix", "infix", "infixl", "infixr", "postfix",
  "notation", "open", "namespace", "section", "theorem", "instance", "at",
  "by", "have", "show", "from", "in", "for", "return", "macro", "syntax",
  "variable", "example", "abbrev", "inductive", "deriving", "with", "axiom"
]

private def kw (n : String) : String :=
  if reserved.contains n then s!"«{n}»" else n

private def strip : Expr → Expr
  | .Annotated x _ => strip x
  | other => other

private def isUpperAscii (s : String) : Bool :=
  !s.isEmpty && s.toList.all (fun c => 'A' ≤ c && c ≤ 'Z')

private def escapeLeanStr (s : String) : String :=
  let inner := s.toList.foldl (fun acc c =>
    match c with
    | '"'  => acc ++ "\\\""
    | '\\' => acc ++ "\\\\"
    | '\n' => acc ++ "\\n"
    | '\t' => acc ++ "\\t"
    | _    => acc.push c) ""
  s!"\"{inner}\""

/-- Parse a decimal literal (e.g. `"0.95"`, `"1"`, `"0.05"`) into integer percent (`95`, `100`, `5`). -/
private def parsePercent (s : String) : Option Int :=
  match s.splitOn "." with
  | [whole] => whole.toInt?.map (· * 100)
  | [whole, frac] =>
    let w := whole.toInt?.getD 0
    let padded := (frac ++ "00").take 2
    padded.toInt?.map (fun f => w * 100 + f)
  | _ => none

/-- Extract Column-2 percentage weights from a form spec (e.g. ARF 210.1A). -/
def formWeights (f : FormDecl) : Measure :=
  let rows := f.items.filterMap fun it =>
    match it.cells.find? (fun (c, _) => c == "weight") with
    | some (_, .Expr e) =>
      match strip e with
      | .Num n _ => (parsePercent n).map (fun p => (it.id, p))
      | _ => none
    | _ => none
  let sorted := rows.mergeSort (fun a b => a.1 ≤ b.1)
  .Weight sorted

/-- Extract NSFR percentage factors `(item, bucketIdx, pct)` from a form spec (ARF 210.6). -/
def formFactors (f : FormDecl) : Measure :=
  let rows := f.items.flatMap fun it =>
    let bCells := [("f0", 0), ("f1", 1), ("f2", 2), ("factor", 9)].filterMap fun (col, idx) =>
      match it.cells.find? (fun (c, _) => c == col) with
      | some (_, .Expr e) =>
        match strip e with
        | .Num n _ => (parsePercent n).map (fun p => (it.id, idx, p))
        | _ => none
      | _ => none
    -- Special case: ARF 210.6 item 8.1/8.2 feed item 8 (50% ASF, single-column bucket 9)
    let extra := if it.id == "8.1" || it.id == "8.2" then [(it.id, 9, (50 : Int))] else []
    bCells ++ extra
  let sorted := rows.mergeSort (fun a b => a.1 < b.1 || (a.1 == b.1 && a.2.1 ≤ b.2.1))
  .Factor sorted

private partial def formCellExpr : Expr → String
  | .Annotated x _ => formCellExpr x
  | .Cell item _ _ => s!"(cell {escapeLeanStr item})"
  | .Num n _ => s!"({n} : Int)"
  | .Unary .Neg x => s!"(-{formCellExpr x})"
  | .Binary op a b =>
    let sym := match op with
      | .Add => "+"
      | .Sub => "-"
      | .Mul => "*"
      | _ => "+"
    s!"({formCellExpr a} {sym} {formCellExpr b})"
  | .Call (.Ident fn _) args =>
    let fnName := if fn == "max" then "max" else "min"
    let vals := args.map (fun (.mk _ v) => formCellExpr v)
    match vals with
    | [] => "(0 : Int)"
    | x :: xs => xs.foldl (fun acc y => s!"({fnName} {acc} {y})") x
  | _ => "(0 : Int)"

/-- Compile derived column-1 (`amount`) form cells to Lean functions `def <name> (cell : String → Int) : Int`. -/
def formCells (f : FormDecl) (specs : List (String × String)) : String :=
  let parts := specs.filterMap fun (itemId, fnName) =>
    match f.items.find? (fun it => it.id == itemId) with
    | some it =>
      match it.cells.find? (fun (c, _) => c == "amount") with
      | some (_, .Expr e) =>
        some s!"/-- {f.code} item {itemId} (generated from the form spec). -/\ndef {fnName} (cell : String → Int) : Int :=\n  {formCellExpr e}\n"
      | _ => none
    | none => none
  String.intercalate "\n" parts

private structure Ctx where
  m              : Module
  enums          : List (String × List String)
  ctorTypes      : List (String × List String)
  contractOf     : List (String × String)
  contractFields : List (String × List (String × Ty))
  events         : List (String × List Field)
  data           : List (String × List (String × Ty))
  cyclic         : List ((String × String) × String)
  params         : List String
  fns            : List String
  worldFns       : List String
  moded          : List String
  numTy          : String
  inOutput       : Bool := false

private def lookup {α : Type} (xs : List (String × α)) (k : String) : Option α :=
  xs.find? (fun (a, _) => a == k) |>.map Prod.snd

private def dataNames (c : Ctx) : List String := c.data.map Prod.fst

private def isDataTy (c : Ctx) (t : Ty) : Option String :=
  match t with
  | .Named name _ _ _ => if (dataNames c).contains name then some name else none
  | _ => none

private def hasRateTy : Ty → Bool
  | .Named "Rate" _ _ _ => true
  | _ => false

private def collectUnionNames : Ty → List String
  | .Union alts => alts.filterMap fun
      | .Named n _ _ _ => some n
      | _ => none
  | _ => []

private def extractContractAlts : Ty → List (String × List (String × Ty))
  | .Named "Contract" generics _ _ =>
    generics.flatMap fun gt =>
      let alts := match gt with
        | .Union xs => xs
        | other => [other]
      alts.filterMap fun
        | .Named v _ params _ =>
          let fs := params.filterMap fun
            | .Field fn fty => some (fn, fty)
            | _ => none
          some (v, fs)
        | _ => none
  | _ => []

private def buildCtx (m : Module) : Ctx :=
  let numTy :=
    if m.decls.any fun d =>
      match d.kind with
      | .Data _ fs => fs.any (fun f => hasRateTy f.ty)
      | .Param ty _ => hasRateTy ty
      | _ => false
    then "Rat" else "Int"
  let enums := m.decls.filterMap fun d =>
    match d.kind with
    | .TypeAlias (.Union alts) => some (d.name, collectUnionNames (.Union alts))
    | _ => none
  let enums := enums.mergeSort (fun a b => a.1 ≤ b.1)
  let allCtorPairs := enums.flatMap fun (en, cs) => cs.map (fun c => (c, en))
  let ctorNames := (allCtorPairs.map Prod.fst).eraseDups
  let ctorTypes := ctorNames.map fun cn =>
    (cn, (allCtorPairs.filter (fun (c, _) => c == cn)).map Prod.snd)
  let contractPairs := m.decls.flatMap fun d =>
    match d.kind with
    | .Data (some base) _ =>
      let ind := s!"Contract_{d.name}"
      (extractContractAlts base).map (fun (v, fs) => (v, ind, fs))
    | _ => []
  let contractOf := contractPairs.map (fun (v, ind, _) => (v, ind))
  let contractFields := contractPairs.map (fun (v, _, fs) => (v, fs))
  let data := m.decls.filterMap fun d =>
    match d.kind with
    | .Data base fs =>
      let baseFs := fs.map (fun f => (f.name, f.ty))
      let withC := match base with
        | some (.Named "Contract" _ _ _) =>
          baseFs ++ [("contract", .Named s!"Contract_{d.name}" [] [] {})]
        | _ => baseFs
      some (d.name, withC)
    | _ => none
  let data := data.mergeSort (fun a b => a.1 ≤ b.1)
  let dNames := data.map Prod.fst
  let cyclic := data.flatMap fun (dn, fs) =>
    fs.filterMap fun (fn, ty) =>
      match ty with
      | .Named "List" [.Named inner _ _ _] _ _ =>
        if dNames.contains inner then some ((dn, fn), s!"{dn}_{fn}") else none
      | _ => none
  let events := m.decls.filterMap fun d =>
    match d.kind with
    | .Event fs => some (d.name, fs)
    | _ => none
  let events := events.mergeSort (fun a b => a.1 ≤ b.1)
  let params := m.decls.filterMap fun d =>
    match d.kind with
    | .Param _ _ => some d.name
    | _ => none
  { m, enums, ctorTypes, contractOf, contractFields, events, data, cyclic,
    params, fns := [], worldFns := [], moded := [], numTy }

private partial def collectCalls : Expr → List String
  | .Annotated x _ | .Unary _ x | .Member x _ | .Is x _ => collectCalls x
  | .Binary _ a b | .Index a b => collectCalls a ++ collectCalls b
  | .List xs | .Tuple xs => xs.flatMap collectCalls
  | .If c t e => collectCalls c ++ collectCalls t ++ collectCalls e
  | .IfLet _ v t e => collectCalls v ++ collectCalls t ++ collectCalls e
  | .Let _ v b => collectCalls v ++ collectCalls b
  | .Match s arms => collectCalls s ++ arms.flatMap (fun (.mk _ g b) => (g.map collectCalls |>.getD []) ++ collectCalls b)
  | .Comprehension _ src flt body =>
    collectCalls src ++ (flt.map collectCalls |>.getD []) ++ (body.map collectCalls |>.getD [])
  | .Group src key flt fs =>
    collectCalls src ++ collectCalls key ++ (flt.map collectCalls |>.getD []) ++ fs.flatMap (fun (_, e) => collectCalls e)
  | .Optimize _ flt _ obj cs =>
    (flt.map collectCalls |>.getD []) ++ collectCalls obj ++ cs.flatMap collectCalls
  | .Forall _ _ b => collectCalls b
  | .Call f args =>
    let fn := match strip f with | .Ident n _ => [n] | _ => []
    fn ++ collectCalls f ++ args.flatMap (fun (.mk _ v) => collectCalls v)
  | _ => []

private partial def collectTableRefs (dNames : List String) : Expr → List String
  | .Annotated x _ | .Unary _ x | .Member x _ | .Is x _ => collectTableRefs dNames x
  | .Binary .In a b =>
    let hit := match strip b with | .Ident n _ => if dNames.contains n then [n] else [] | _ => []
    hit ++ collectTableRefs dNames a ++ collectTableRefs dNames b
  | .Binary _ a b | .Index a b => collectTableRefs dNames a ++ collectTableRefs dNames b
  | .List xs | .Tuple xs => xs.flatMap (collectTableRefs dNames)
  | .If c t e => collectTableRefs dNames c ++ collectTableRefs dNames t ++ collectTableRefs dNames e
  | .IfLet _ v t e => collectTableRefs dNames v ++ collectTableRefs dNames t ++ collectTableRefs dNames e
  | .Let _ v b => collectTableRefs dNames v ++ collectTableRefs dNames b
  | .Match s arms => collectTableRefs dNames s ++ arms.flatMap (fun (.mk _ g b) => (g.map (collectTableRefs dNames) |>.getD []) ++ collectTableRefs dNames b)
  | .Comprehension _ src flt body =>
    let hit := match strip src with | .Ident n _ => if dNames.contains n then [n] else [] | _ => []
    hit ++ collectTableRefs dNames src ++ (flt.map (collectTableRefs dNames) |>.getD []) ++ (body.map (collectTableRefs dNames) |>.getD [])
  | .Call f args => collectTableRefs dNames f ++ args.flatMap (fun (.mk _ v) => collectTableRefs dNames v)
  | _ => []

private partial def usesMode (m : Module) (seen : List String) (e : Expr) : Bool :=
  let rec walk : Expr → Bool
    | .Annotated x _ | .Unary _ x | .Member x _ | .Is x _ => walk x
    | .Binary _ a b | .Index a b => walk a || walk b
    | .List xs | .Tuple xs => xs.any walk
    | .If c t e => walk c || walk t || walk e
    | .IfLet _ v t e => walk v || walk t || walk e
    | .Let _ v b => walk v || walk b
    | .Match s arms => walk s || arms.any (fun (.mk _ g b) => (g.map walk |>.getD false) || walk b)
    | .Comprehension _ src flt body => walk src || (flt.map walk |>.getD false) || (body.map walk |>.getD false)
    | .Group src key flt fs => walk src || walk key || (flt.map walk |>.getD false) || fs.any (fun (_, x) => walk x)
    | .Optimize _ _ _ _ _ => true
    | .Forall _ _ b => walk b
    | .Call f args =>
      let fHit := match strip f with
        | .Ident "conservative" _ => true
        | .Ident n _ =>
          if seen.contains n then false
          else match m.decls.find? (fun d => d.name == n) with
            | some { kind := .Fn _ _ (some b), .. } => usesMode m (n :: seen) b
            | _ => false
        | _ => false
      fHit || args.any (fun (.mk _ v) => walk v)
    | .Ident n _ =>
      if seen.contains n then false
      else match m.decls.find? (fun d => d.name == n) with
        | some { kind := .View b, .. } => usesMode m (n :: seen) b
        | some { kind := .Alloc _ _, .. } => true
        | _ => false
    | _ => false
  walk e

private partial def compileTy (numTy : String) : Ty → String
  | .Union _ => "Unit"
  | .Named name generics _ _ =>
    let g0 := match generics.head? with | some t => compileTy numTy t | none => "Unit"
    match name with
    | "Bool" => "Bool"
    | "String" => "String"
    | "Int" | "Date" => "Int"
    | "Rate" | "Money" => numTy
    | "GroupId" | "ProductClass" | "Mode" => name
    | "Uncertain" => s!"U ({g0})"
    | "Option" => s!"Option ({g0})"
    | "List" => s!"List ({g0})"
    | other => other

private partial def bindNames : Pat → List String
  | .Wild => []
  | .Bind n => [n]
  | .Ctor _ ps | .Tuple ps => ps.flatMap bindNames

private def isCtor (c : Ctx) (n : String) : Bool :=
  (lookup c.ctorTypes n).isSome || (lookup c.contractOf n).isSome

private def renderCtor (c : Ctx) (n : String) (qualified : Bool) : String :=
  match lookup c.contractOf n with
  | some ind => if qualified then s!"{ind}.{n}" else s!".{n}"
  | none =>
    match lookup c.ctorTypes n with
    | some [t] => if qualified then s!"{t}.{n}" else s!".{n}"
    | _ => s!".{n}"

private partial def renderPat (c : Ctx) : Pat → String
  | .Wild => "_"
  | .Bind n => kw n
  | .Tuple ps => s!"({String.intercalate ", " (ps.map (renderPat c))})"
  | .Ctor "Some" [p] => renderPat c p
  | .Ctor name ps =>
    match lookup c.events name with
    | some evFs =>
      let rendered := ps.map (renderPat c)
      let pad := List.replicate (evFs.length - rendered.length) "_"
      let all := rendered ++ pad ++ ["_", "_"]
      s!"⟨{String.intercalate ", " all}⟩"
    | none =>
      let head := renderCtor c name false
      let args := ps.map (fun p => " " ++ renderPat c p)
      head ++ String.join args

private def worldRef (c : Ctx) (n : String) : String :=
  if c.moded.contains n then s!"(W.{n} m)" else s!"W.{n}"

private def declOf (c : Ctx) (n : String) : Option Decl :=
  c.m.decls.find? (fun d => d.name == n)

private partial def compileExpr (c : Ctx) (locals : List String) (e : Expr) : Except String String := do
  match e with
  | .Annotated x _ => compileExpr c locals x
  | .Num n _ =>
    if n.contains '.' then throw s!"non-integer literal {n}"
    else if c.numTy == "Int" then pure s!"({n} : Int)"
    else pure n
  | .Percent n _ =>
    if n.contains '.' then throw s!"non-integer percent {n}"
    else pure s!"({n} : Int)"
  | .Str s _ => pure (escapeLeanStr s)
  | .Ident n _ =>
    if locals.contains n then pure (kw n)
    else if n == "true" || n == "false" || n == "none" then pure n
    else if n == "Conservative" || n == "Optimistic" then pure s!"Mode.{n}"
    else if c.params.contains n then pure s!"W.{n}"
    else if c.worldFns.contains n then pure (worldRef c n)
    else if isCtor c n then pure (renderCtor c n true)
    else if (dataNames c).contains n then pure s!"W.{n}_all"
    else throw s!"cannot compile name `{n}`"
  | .List xs =>
    let ys ← xs.mapM (compileExpr c locals)
    pure s!"[{String.intercalate ", " ys}]"
  | .Tuple xs =>
    let ys ← xs.mapM (compileExpr c locals)
    pure s!"({String.intercalate ", " ys})"
  | .Unary .Not x =>
    let y ← compileExpr c locals x
    pure s!"(!{y})"
  | .Unary .Neg x =>
    let y ← compileExpr c locals x
    pure s!"(-{y})"
  | .Member x f =>
    match strip x, f with
    | .Num n _, u =>
      if isUpperAscii u then pure s!"({n} : {c.numTy})"
      else if u == "days" then compileExpr c locals x
      else do
        let obj ← compileExpr c locals x
        pure s!"{obj}.{f}"
    | .Unary .Neg inner, u =>
      if isUpperAscii u then do
        let y ← compileExpr c locals inner
        pure s!"(-{y})"
      else do
        let obj ← compileExpr c locals x
        pure s!"{obj}.{f}"
    | _, "days" => compileExpr c locals x
    | _, _ =>
      let obj ← compileExpr c locals x
      match c.cyclic.find? (fun ((_, fld), _) => fld == f) with
      | some (_, wf) => pure s!"(W.{wf} {obj})"
      | none => pure s!"{obj}.{f}"
  | .Index x k =>
    match strip x with
    | .Ident n _ =>
      if c.worldFns.contains n && !locals.contains n then do
        let kv ← compileExpr c locals k
        pure s!"({worldRef c n} {kv})"
      else throw "indexing only supported on views/allocs"
    | _ => throw "indexing only supported on views/allocs"
  | .Binary op a b =>
    let l ← compileExpr c locals a
    let r ← compileExpr c locals b
    match op with
    | .And => pure s!"({l} && {r})"
    | .Or => pure s!"({l} || {r})"
    | .Implies => pure s!"(!{l} || {r})"
    | .Eq => pure s!"({l} == {r})"
    | .Ne => pure s!"({l} != {r})"
    | .Lt => pure s!"(pcmp (· < ·) {l} {r})"
    | .Le => pure s!"(pcmp (· ≤ ·) {l} {r})"
    | .Gt => pure s!"(pcmp (· > ·) {l} {r})"
    | .Ge => pure s!"(pcmp (· ≥ ·) {l} {r})"
    | .In => pure s!"(List.contains {r} {l})"
    | .Add => pure s!"({l} + {r})"
    | .Sub => pure s!"({l} - {r})"
    | .Mul => pure s!"({l} * {r})"
    | .Div =>
      if c.numTy == "Rat" then pure s!"({l} / {r})"
      else throw "division not compiled in Int model"
  | .If cond t e =>
    let cv ← compileExpr c locals cond
    let tv ← compileExpr c locals t
    let ev ← compileExpr c locals e
    pure s!"(if {cv} then {tv} else {ev})"
  | .IfLet pat val t e =>
    let vv ← compileExpr c locals val
    let tv ← compileExpr c (bindNames pat ++ locals) t
    let ev ← compileExpr c locals e
    pure s!"(match {vv} with\n    | some {renderPat c pat} => {tv}\n    | _ => {ev})"
  | .Let (.Bind n) val body =>
    let vv ← compileExpr c locals val
    let bv ← compileExpr c (n :: locals) body
    pure s!"(let {kw n} := {vv}\n    {bv})"
  | .Let _ _ _ => throw "pattern let not compiled"
  | .Match scr arms =>
    let sv ← compileExpr c locals scr
    compileMatch c locals sv arms
  | .Call f args => compileCall c locals f args
  | _ => throw "unsupported expr in Lean model"
where
  compileMatch (c : Ctx) (locals : List String) (scrut : String) (arms : List Arm) : Except String String := do
    let rec go (skip : List String) : List Arm → Except String (List String)
      | [] => pure []
      | (.mk pat guard body) :: rest => do
        let pStr := renderPat c pat
        if skip.contains pStr then go skip rest
        else
          let inner := bindNames pat ++ locals
          let bStr ← compileExpr c inner body
          match guard with
          | none =>
            let tail ← go skip rest
            pure (s!"\n    | {pStr} => {bStr}" :: tail)
          | some g =>
            let gStr ← compileExpr c inner g
            let restStr ← compileMatch c locals scrut rest
            let trimmed := String.ofList (restStr.toList.dropLast) ++ "\n    | _ => default)"
            let line := s!"\n    | {pStr} => if {gStr} then {bStr} else {trimmed}"
            let tail ← go (pStr :: skip) rest
            pure (line :: tail)
    let lines ← go [] arms
    pure s!"(match {scrut} with{String.join lines})"

  compileCall (c : Ctx) (locals : List String) (f : Expr) (args : List Arg) : Except String String := do
    let .Ident name _ := strip f | throw "only named calls"
    let named := fun (k : String) => args.find? (fun (.mk nm _) => nm == some k) |>.map (fun (.mk _ v) => v)
    if c.fns.contains name then
      let ps := match declOf c name with
        | some { kind := .Fn ps _ _, .. } => ps
        | _ => []
      let posArgs := args.filterMap (fun (.mk nm v) => if nm.isNone then some v else none)
      let mut out := s!"({kw name} W m"
      let mut posIdx := 0
      for p in ps do
        let argVal ← match named p.name with
          | some v => pure v
          | none =>
            match posArgs[posIdx]? with
            | some v => posIdx := posIdx + 1; pure v
            | none => throw s!"missing arg {p.name}"
        let av ← compileExpr c locals argVal
        out := out ++ " " ++ av
      return out ++ ")"
    if c.worldFns.contains name then
      let as ← args.mapM (fun (.mk _ v) => compileExpr c locals v)
      return s!"({worldRef c name} {String.intercalate " " as})"
    let compHelper := fun (fn : String → String → Option String → String → String) => do
      match args with
      | [.mk _ (.Comprehension var src flt body)] =>
        let inner := var :: locals
        let sStr ← compileExpr c locals src
        let fStr ← match flt with | some fe => some <$> compileExpr c inner fe | none => pure none
        let bStr ← match body with | some be => compileExpr c inner be | none => pure (kw var)
        pure (fn (kw var) sStr fStr bStr)
      | _ => throw s!"{name}(..) needs comprehension"
    let filtSrc := fun (v src : String) (flt : Option String) =>
      match flt with
      | some cond => s!"({src}.filter (fun {v} => {cond}))"
      | none => src
    match name with
    | "conservative" =>
      let x ← match args.head? with | some (.mk _ v) => compileExpr c locals v | none => throw "conservative arg"
      let worst ← match named "worst" with | some v => pure v | none => throw "conservative worst"
      let best := named "best"
      let isNone := fun (e : Expr) => match strip e with | .Ident "none" _ => true | _ => false
      if isNone worst || (best.map isNone |>.getD false) then
        let lift := fun (o : Option Expr) => do
          match o with
          | some e => if isNone e then pure "none" else pure s!"(some {← compileExpr c locals e})"
          | none => throw "conservative with none needs best"
        pure s!"(resolve m (U.map some {x}) {← lift (some worst)} {← lift best})"
      else
        let w ← compileExpr c locals worst
        let b ← match best with | some be => compileExpr c locals be | none => pure s!"(!{w})"
        pure s!"(resolve m {x} {w} {b})"
    | "days" =>
      match args with
      | [.mk _ a, .mk _ b] => pure s!"({← compileExpr c locals b} - {← compileExpr c locals a})"
      | _ => throw "days needs 2 args"
    | "item" =>
      match args with
      | [.mk _ a, .mk _ b] =>
        let av ← compileExpr c locals a
        match strip b with
        | .Num n _ =>
          if !n.contains '.' then pure s!"({av} ++ \".{n}\")"
          else pure s!"(s!\"\{{av}}.\{{← compileExpr c locals b}}\")"
        | _ => pure s!"(s!\"\{{av}}.\{{← compileExpr c locals b}}\")"
      | _ => throw "item needs 2 args"
    | "bucket" =>
      match args with
      | [.mk _ a] =>
        let av ← compileExpr c locals a
        if c.inOutput then pure s!"(W.bucket {av})"
        else pure s!"(bucketName (W.bucket {av}))"
      | _ => throw "bucket needs 1 arg"
    | "add_months" =>
      match args with
      | [.mk _ a, .mk _ b] => pure s!"(addMonths {← compileExpr c locals a} {← compileExpr c locals b})"
      | _ => throw "add_months needs 2 args"
    | "cycle" =>
      match args with
      | [.mk _ a, .mk _ b, .mk _ d] =>
        pure s!"(cycle {← compileExpr c locals a} {← compileExpr c locals b} {← compileExpr c locals d})"
      | _ => throw "cycle needs 3 args"
    | "pow" =>
      match args with
      | [.mk _ a, .mk _ b] => pure s!"(qpow {← compileExpr c locals a} {← compileExpr c locals b})"
      | _ => throw "pow needs 2 args"
    | "solo" =>
      match args with
      | [.mk _ a] => pure s!"(W.solo {← compileExpr c locals a})"
      | _ => throw "solo needs 1 arg"
    | "min" | "max" =>
      match args with
      | [.mk _ a, .mk _ b] => pure s!"({name} {← compileExpr c locals a} {← compileExpr c locals b})"
      | [_] => compHelper fun v src flt body =>
        s!"(match List.map (fun {v} => {body}) {filtSrc v src flt} with | [] => 0 | x :: xs => xs.foldl {name} x)"
      | _ => throw s!"{name} arity"
    | "all" => compHelper fun v src flt body => s!"(List.all {filtSrc v src flt} (fun {v} => {body}))"
    | "distinct" => compHelper fun v src flt body => s!"(List.eraseDups (List.map (fun {v} => {body}) {filtSrc v src flt}))"
    | "sum" => compHelper fun v src flt body => s!"(List.foldr (· + ·) 0 (List.map (fun {v} => {body}) {filtSrc v src flt}))"
    | other => throw s!"built-in `{other}` is not compiled"

private def preludeStr : String :=
"inductive Mode where
  | Conservative
  | Optimistic
  deriving DecidableEq, Repr, Inhabited

/-- `Uncertain<T>` -/
inductive U (α : Type) where
  | known (a : α)
  | unknown
  deriving DecidableEq, Repr, Inhabited

def U.map {α β : Type} (f : α → β) : U α → U β
  | .known a => .known (f a)
  | .unknown => .unknown

/-- `conservative(x, worst: w, best: b)` -/
def resolve {α : Type} (m : Mode) (x : U α) (worst best : α) : α :=
  match x with
  | .known a => a
  | .unknown => match m with
    | .Conservative => worst
    | .Optimistic => best

/-- Ordered comparison where an absent optional date compares false. -/
class ToOpt (α : Type) where
  toOpt : α → Option Int
instance : ToOpt Int := ⟨some⟩
instance : ToOpt (Option Int) := ⟨id⟩
instance : ToOpt Nat := ⟨fun n => some (n : Int)⟩
def pcmp {α β : Type} [ToOpt α] [ToOpt β] (r : Int → Int → Prop) [DecidableRel r] (a : α) (b : β) : Bool :=
  match ToOpt.toOpt a, ToOpt.toOpt b with
  | some x, some y => decide (r x y)
  | _, _ => false

abbrev GroupId := Nat
abbrev ProductClass := Nat

"

private def calendarStr : String :=
"/-- Days since 1970-01-01 of a proleptic Gregorian date. -/
def daysFromCivil (y m d : Int) : Int :=
  let y := if m ≤ 2 then y - 1 else y
  let era := Int.tdiv (if y ≥ 0 then y else y - 399) 400
  let yoe := y - era * 400
  let doy := Int.tdiv (153 * (if m > 2 then m - 3 else m + 9) + 2) 5 + d - 1
  let doe := yoe * 365 + Int.tdiv yoe 4 - Int.tdiv yoe 100 + doy
  era * 146097 + doe - 719468

def civilFromDays (z : Int) : Int × Int × Int :=
  let z := z + 719468
  let era := Int.tdiv (if z ≥ 0 then z else z - 146096) 146097
  let doe := z - era * 146097
  let yoe := Int.tdiv (doe - Int.tdiv doe 1460 + Int.tdiv doe 36524 - Int.tdiv doe 146096) 365
  let y := yoe + era * 400
  let doy := doe - (365 * yoe + Int.tdiv yoe 4 - Int.tdiv yoe 100)
  let mp := Int.tdiv (5 * doy + 2) 153
  let d := doy - Int.tdiv (153 * mp + 2) 5 + 1
  let m := if mp < 10 then mp + 3 else mp - 9
  (if m ≤ 2 then y + 1 else y, m, d)

def daysInMonth (y m : Int) : Int :=
  if m = 2 then (if (y % 4 = 0 ∧ y % 100 ≠ 0) ∨ y % 400 = 0 then 29 else 28)
  else if m = 4 ∨ m = 6 ∨ m = 9 ∨ m = 11 then 30 else 31

/-- `add_months`: shift by whole months, clamping the day to the month's length. -/
def addMonths (day months : Int) : Int :=
  match civilFromDays day with
  | (y, m, d) =>
    let total := (m - 1) + months
    let y2 := y + Int.ediv total 12
    let m2 := Int.emod total 12 + 1
    daysFromCivil y2 m2 (min d (daysInMonth y2 m2))

/-- ACTUS cycle: the payment numbers k ≥ 0 with addMonths start (k * months) ≤ upto
    (months have at least 28 days, so k ≤ (upto - start) / 28). -/
def cycle (start months upto : Int) : List Int :=
  ((List.range (Int.toNat ((upto - start) / 28 + 1))).map Int.ofNat).filter
    (fun k => decide (addMonths start (k * months) ≤ upto))

"

private def powStr : String :=
"def qpowN (x : Rat) : Nat → Rat
  | 0 => 1
  | n + 1 => qpowN x n * x

def qpow (x : Rat) (n : Int) : Rat :=
  if n ≥ 0 then qpowN x n.toNat else 1 / qpowN x (-n).toNat

"

private def replaceAll (s pat rep : String) : String :=
  String.intercalate rep (s.splitOn pat)

private def splitCombos : Nat → List String
  | 0 => [""]
  | n + 1 =>
    let sub := splitCombos n
    sub.map (" true" ++ ·) ++ sub.map (" false" ++ ·)

/-- Compile a `Prudence.DSL.Module` to its executable Lean 4 proof model source. -/
def generateModel (m : Module) (targets : List (String × Measure)) (extraTail : String := "") : String := Id.run do
  let mut c := buildCtx m
  let mut out := s!"/-\n  GENERATED by `lake exe prudence-export` from `Prudence.Specs.Modules` (`{m.name}`). Do not edit.\n-/\n"
  out := out ++ "set_option autoImplicit false\nset_option linter.unusedVariables false\nset_option match.ignoreUnusedAlts true\n\n"
  out := out ++ s!"namespace {m.name}\n\n" ++ preludeStr

  let allCalls := (m.decls.flatMap fun d =>
    match d.kind with
    | .Fn _ _ (some b) | .View b | .Invariant b => collectCalls b
    | .Partition _ _ _ sp _ spb ms cs =>
      collectCalls sp ++ collectCalls spb ++ ms.flatMap (fun (_, e) => collectCalls e) ++
      cs.flatMap (fun k => collectCalls k.cond ++ k.lets.flatMap (fun (_, e) => collectCalls e) ++ k.outputs.flatMap (fun (_, e) => collectCalls e))
    | _ => []).eraseDups
  if allCalls.contains "add_months" || allCalls.contains "cycle" then out := out ++ calendarStr
  if allCalls.contains "pow" then out := out ++ powStr
  if allCalls.contains "bucket" then
    out := out ++ "/-- The interpreter's name of an NSFR maturity bucket. -/\ndef bucketName : Nat → String\n  | 0 => \"b0\"\n  | 1 => \"b1\"\n  | _ => \"b2\"\n\n"

  -- Enums
  for (en, cs) in c.enums do
    let alts := String.intercalate "\n" (cs.map (fun x => s!"  | {x}"))
    out := out ++ s!"inductive {en} where\n{alts}\n  deriving DecidableEq, Repr, Inhabited\n\n"

  -- Records in dependency order (contracts before their owner)
  let dNames := dataNames c
  let mut done : List String := []
  let mut pending := dNames
  for _ in List.range (dNames.length + 1) do
    if pending.isEmpty then break
    let mut nextPending : List String := []
    for dn in pending do
      let fs := (lookup c.data dn).getD []
      let deps := fs.filterMap fun (fn, ty) =>
        if c.cyclic.any (fun ((d, f), _) => d == dn && f == fn) then none
        else match ty with
          | .Named n _ _ _ => if dNames.contains n && n != dn then some n else none
          | _ => none
      if deps.any (fun d => !done.contains d) then
        nextPending := nextPending ++ [dn]
      else
        if let some (_, .Named ind _ _ _) := fs.find? (fun (f, _) => f == "contract") then
          let ctors := ((c.contractOf.filter (fun (_, i) => i == ind)).map Prod.fst).mergeSort (· ≤ ·)
          out := out ++ s!"inductive {ind} where\n"
          for ct in ctors do
            let cfs := (lookup c.contractFields ct).getD []
            let fStrs := cfs.map (fun (fn, fty) => s!" ({kw fn} : {compileTy c.numTy fty})")
            out := out ++ s!"  | {ct}{String.join fStrs}\n"
          out := out ++ "  deriving DecidableEq, Repr, Inhabited\n\n"
        out := out ++ s!"structure {dn} where\n"
        for (fn, ty) in fs do
          if c.cyclic.any (fun ((d, f), _) => d == dn && f == fn) then
            out := out ++ s!"  -- {fn}: world function W.{dn}_{fn} (would make records cyclic)\n"
          else
            out := out ++ s!"  {kw fn} : {compileTy c.numTy ty}\n"
        out := out ++ "  deriving DecidableEq, Repr, Inhabited\n\n"
        done := dn :: done
    pending := nextPending

  -- Events
  for (ev, fs) in c.events do
    out := out ++ s!"structure {ev} where\n"
    for f in fs do
      out := out ++ s!"  {kw f.name} : {compileTy c.numTy f.ty}\n"
    out := out ++ "  valid : Int\n  recorded : Int\n  deriving DecidableEq, Repr, Inhabited\n\n"

  -- Mode dependence
  let mut moded : List String := []
  for d in m.decls do
    let isM := match d.kind with
      | .View b => usesMode m [] b
      | .Alloc _ _ => true
      | .Fn _ _ (some b) => usesMode m [d.name] b
      | _ => false
    if isM then moded := d.name :: moded
  c := { c with moded := moded }

  -- World fields
  let mut worldFields : List String := []
  for d in m.decls do
    if let .Param ty _ := d.kind then
      worldFields := worldFields ++ [s!"  {d.name} : {compileTy c.numTy ty}"]

  let mut viewRows := ""
  for d in m.decls do
    match d.kind with
    | .View body =>
      let mPfx := if c.moded.contains d.name then "Mode → " else ""
      let mut vTy := c.numTy
      match strip body with
      | .Group _ _ _ fs =>
        let rowName := s!"{d.name}_row"
        viewRows := viewRows ++ s!"structure {rowName} where\n"
        for (fn, _) in fs do
          viewRows := viewRows ++ s!"  {fn} : {c.numTy}\n"
        viewRows := viewRows ++ "  deriving Repr, Inhabited\n\n"
        vTy := s!"Option (GroupId) → {rowName}"
      | .Call (.Ident "distinct" _) _ => vTy := "List (ProductClass)"
      | _ => ()
      worldFields := worldFields ++ [s!"  {d.name} : {mPfx}{vTy}  -- view"]
      c := { c with worldFns := d.name :: c.worldFns }
    | .Alloc (.Named _ _ params _) _ =>
      let (src, fld) := match params.head? with
        | some (.Expr (.Member (.Ident s _) f)) => (s, f)
        | _ => ("Deposit", "balance")
      let vt := ((lookup c.data src).getD []).find? (fun (f, _) => f == fld)
        |>.map (fun (_, t) => compileTy c.numTy t) |>.getD c.numTy
      worldFields := worldFields ++ [s!"  {d.name} : Mode → {src} → {vt}  -- optimize solution"]
      c := { c with worldFns := d.name :: c.worldFns }
    | _ => ()

  for ((dn, fn), wf) in c.cyclic do
    let ty := ((lookup c.data dn).getD []).find? (fun (f, _) => f == fn)
      |>.map (fun (_, t) => compileTy c.numTy t) |>.getD "Unit"
    worldFields := worldFields ++ [s!"  {wf} : {dn} → {ty}  -- cyclic record field"]

  let tables := ((m.decls.flatMap fun d =>
    match d.kind with | .Fn _ _ (some b) => collectTableRefs dNames b | _ => []).eraseDups).mergeSort (· ≤ ·)
  for t in tables do
    worldFields := worldFields ++ [s!"  {t}_all : List {t}  -- every {t} record"]

  if allCalls.contains "bucket" then
    worldFields := worldFields ++ ["  bucket : Int → Nat  -- NSFR maturity bucket of a date (built-in)"]
  if allCalls.contains "solo" then
    let owner := (c.data.find? (fun (_, fs) => fs.any (fun (f, _) => f == "sme_group"))).map Prod.fst |>.getD "Customer"
    worldFields := worldFields ++ [s!"  solo : {owner} → GroupId  -- the group of just one {owner} (built-in)"]

  out := out ++ viewRows

  -- Functions
  let mut fnsOut := ""
  for d in m.decls do
    if let .Fn ps ret (some body) := d.kind then
      let locals := ps.map (·.name)
      let cTry := { c with fns := d.name :: c.fns }
      match compileExpr cTry locals body with
      | .ok bStr =>
        c := cTry
        let pStr := String.join (ps.map (fun p => s!" ({kw p.name} : {compileTy c.numTy p.ty})"))
        fnsOut := fnsOut ++ s!"def {kw d.name} (W : World) (m : Mode){pStr} : {compileTy c.numTy ret} :=\n  {bStr}\n\n"
      | .error err =>
        c := { c with worldFns := d.name :: c.worldFns }
        let mPfx := if c.moded.contains d.name then "Mode → " else ""
        let pTys := String.intercalate " → " (ps.map (fun p => compileTy c.numTy p.ty))
        let sig := if ps.isEmpty then compileTy c.numTy ret else s!"{pTys} → {compileTy c.numTy ret}"
        worldFields := worldFields ++ [s!"  {d.name} : {mPfx}{sig}  -- fn over events/views: {err}"]

  out := out ++ s!"/-- Everything the rules read that is not computed here. -/\nstructure World where\n{String.intercalate "\n" worldFields}\n\n"
  out := out ++ fnsOut

  -- Partition
  match m.decls.find? (fun d => match d.kind with | .Partition .. => true | _ => false) with
  | some { kind := .Partition var srcTy _ split splitVar splitBy moreSplits cases, .. } =>
    let src := compileTy c.numTy srcTy
    let splits := (splitVar, splitBy) :: moreSplits
    let flags := splits.map (fun (n, _) => kw n)
    let binders := String.join (flags.map (fun f => s!" ({f} : Bool)"))
    let mut scope := [var]
    let whole := (compileExpr c scope split).toOption.getD "0"
    let mut pBody := ""
    if splits.length == 1 then
      let a := (compileExpr c scope splitBy).toOption.getD "0"
      pBody := s!"  if {flags.head!} then {a} else {whole} - {a}"
      scope := splitVar :: scope
    else
      pBody := s!"  let p0 := {whole}\n"
      let mut k := 0
      for (sn, amt) in splits do
        let a := (compileExpr c scope amt).toOption.getD "0"
        pBody := pBody ++ s!"  let p{k + 1} := if {kw sn} then {a} else p{k} - ({a})\n"
        scope := sn :: scope
        k := k + 1
      pBody := pBody ++ s!"  p{splits.length}"
    out := out ++ s!"/-- The `partition` portion of `{var}` for the split flags {String.intercalate ", " flags}. -/\ndef portion (W : World) (m : Mode) ({var} : {src}){binders} : {c.numTy} :=\n{pBody}\n\n"

    let outNames := match cases.head? with | some c0 => c0.outputs.map Prod.fst | none => []
    let combos := splitCombos splits.length
    for ((tname, measure), outName) in targets.zip outNames do
      let retTy := match measure with | .Weight _ => "Option String" | .Factor _ => "String × Nat"
      let mut chain := ""
      let mut idx := 0
      for kase in cases do
        let mut inner := scope
        let mut letsStr := ""
        for (lp, le) in kase.lets do
          match lp with
          | .Bind ln =>
            let lv := (compileExpr c inner le).toOption.getD "0"
            letsStr := letsStr ++ s!"let {kw ln} := {lv}\n    "
            inner := ln :: inner
          | _ => ()
        let oe := (kase.outputs.find? (fun (n, _) => n == outName)).map Prod.snd |>.getD (.Ident "none" {})
        let cOut := { c with inOutput := true }
        let compiled := (compileExpr cOut inner oe).toOption.getD "none"
        let val := letsStr ++ compiled
        let condStr := (compileExpr c scope kase.cond).toOption.getD "true"
        if idx + 1 < cases.length then
          chain := chain ++ s!"if {condStr} then\n    {val}\n  else "
        else
          chain := chain ++ s!"\n    -- remaining case (exhaustive by the Z3 obligation): {condStr}\n    {val}"
        idx := idx + 1
      match measure with
      | .Factor _ =>
        chain := replaceAll chain "\"b0\")" "(0 : Nat))"
        chain := replaceAll chain "\"b1\")" "(1 : Nat))"
        chain := replaceAll chain "\"b2\")" "(2 : Nat))"
        chain := replaceAll chain "\"b0\"" "(0 : Nat)"
        chain := replaceAll chain "\"b1\"" "(1 : Nat)"
        chain := replaceAll chain "\"b2\"" "(2 : Nat)"
        chain := replaceAll chain "\"amount\")" "(9 : Nat))"
      | .Weight _ => ()
      out := out ++ s!"def out_{outName} (W : World) (m : Mode) ({var} : {src}){binders} : {retTy} :=\n  {chain}\n\n"
      let mut tblName := ""
      let mut sig := ""
      match measure with
      | .Weight ws =>
        out := out ++ s!"/-- Column-2 weights of {tname} (percent). -/\ndef weight_{tname} : Option String → Int\n"
        for (item, pct) in ws do
          out := out ++ s!"  | some {escapeLeanStr item} => {pct}\n"
        tblName := s!"weight_{tname}"
        sig := "Column-2 weights"
      | .Factor fs =>
        out := out ++ s!"/-- Factors of {tname} by maturity bucket (percent). -/\ndef factor_{tname} : String × Nat → Int\n"
        for (item, b, pct) in fs do
          out := out ++ s!"  | ({escapeLeanStr item}, {b}) => {pct}\n"
        tblName := s!"factor_{tname}"
        sig := "factors"
      out := out ++ "  | _ => 0\n\n"
      let terms := combos.map fun cmb =>
        if c.numTy == "Int" then s!"{tblName} (out_{outName} W m {var}{cmb}) * portion W m {var}{cmb}"
        else s!"({tblName} (out_{outName} W m {var}{cmb}) : {c.numTy}) * portion W m {var}{cmb}"
      out := out ++ s!"/-- {tname} measure of `{var}` ({sig}, percent of the amount). -/\ndef measure_{tname} (W : World) (m : Mode) ({var} : {src}) : {c.numTy} :=\n  {String.intercalate "\n    + " terms}\n\n"
  | _ => ()

  if !extraTail.isEmpty then
    out := out ++ extraTail ++ "\n"
  out := out ++ s!"end {m.name}\n"
  return out

end Prudence.ModelGen
