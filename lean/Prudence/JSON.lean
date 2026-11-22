import Prudence.DSL

/-!
# Prudence IR -> JSON Serializer

Serializes `Prudence.DSL.File` (forms and modules) to the exact `serde_json`
schema expected by `crates/prudence-core/src/syntax/ast.rs`.
-/

set_option autoImplicit false

namespace Prudence.JSON

open Prudence.DSL

def escapeStr (s : String) : String :=
  s.foldl (init := "") fun acc c =>
    match c with
    | '"'  => acc ++ "\\\""
    | '\\' => acc ++ "\\\\"
    | '\n' => acc ++ "\\n"
    | '\r' => acc ++ "\\r"
    | '\t' => acc ++ "\\t"
    | c    =>
      let n := c.toNat
      if n < 0x20 then
        let hex := "0123456789abcdef".toList
        let h1 := hex[(n / 16) % 16]!
        let h2 := hex[n % 16]!
        acc ++ s!"\\u00{h1}{h2}"
      else
        acc.push c

def jStr (s : String) : String := "\"" ++ escapeStr s ++ "\""
def jBool (b : Bool) : String := if b then "true" else "false"
def jNat (n : Nat) : String := toString n
def jArr (xs : List String) : String := "[" ++ String.intercalate "," xs ++ "]"
def jObj (fs : List (String × String)) : String :=
  "{" ++ String.intercalate "," (fs.map fun (k, v) => jStr k ++ ":" ++ v) ++ "}"
def jOpt {α : Type} (f : α → String) : Option α → String
  | none => "null"
  | some x => f x
def jVariant (tag : String) (payload : String) : String :=
  jObj [(tag, payload)]

def posJson (p : Pos) : String :=
  jObj [("line", jNat p.line), ("col", jNat p.col)]

def binOpJson : BinOp → String
  | .Implies => jStr "Implies"
  | .Or      => jStr "Or"
  | .And     => jStr "And"
  | .Eq      => jStr "Eq"
  | .Ne      => jStr "Ne"
  | .Lt      => jStr "Lt"
  | .Le      => jStr "Le"
  | .Gt      => jStr "Gt"
  | .Ge      => jStr "Ge"
  | .In      => jStr "In"
  | .Add     => jStr "Add"
  | .Sub     => jStr "Sub"
  | .Mul     => jStr "Mul"
  | .Div     => jStr "Div"

def unOpJson : UnOp → String
  | .Not => jStr "Not"
  | .Neg => jStr "Neg"

def senseJson : Sense → String
  | .Maximize => jStr "Maximize"
  | .Minimize => jStr "Minimize"

partial def patJson : Pat → String
  | .Wild => jStr "Wild"
  | .Bind s => jVariant "Bind" (jStr s)
  | .Ctor s ps => jVariant "Ctor" (jArr [jStr s, jArr (ps.map patJson)])
  | .Tuple ps => jVariant "Tuple" (jArr (ps.map patJson))

mutual

  partial def exprJson : Expr → String
    | .Num v p => jVariant "Num" (jArr [jStr v, posJson p])
    | .Percent v p => jVariant "Percent" (jArr [jStr v, posJson p])
    | .Str v p => jVariant "Str" (jArr [jStr v, posJson p])
    | .Ident v p => jVariant "Ident" (jArr [jStr v, posJson p])
    | .Cell i c p => jVariant "Cell" (jArr [jStr i, jStr c, posJson p])
    | .List xs => jVariant "List" (jArr (xs.map exprJson))
    | .Tuple xs => jVariant "Tuple" (jArr (xs.map exprJson))
    | .Unary op e => jVariant "Unary" (jArr [unOpJson op, exprJson e])
    | .Binary op a b => jVariant "Binary" (jArr [binOpJson op, exprJson a, exprJson b])
    | .Call f args => jVariant "Call" (jArr [exprJson f, jArr (args.map argJson)])
    | .Index t i => jVariant "Index" (jArr [exprJson t, exprJson i])
    | .Member t f => jVariant "Member" (jArr [exprJson t, jStr f])
    | .Is t c => jVariant "Is" (jArr [exprJson t, jStr c])
    | .If c t e => jVariant "If" (jObj [("cond", exprJson c), ("then", exprJson t), ("other", exprJson e)])
    | .IfLet p v t e => jVariant "IfLet" (jObj [("pat", patJson p), ("value", exprJson v), ("then", exprJson t), ("other", exprJson e)])
    | .Match s arms => jVariant "Match" (jObj [("scrutinee", exprJson s), ("arms", jArr (arms.map armJson))])
    | .Let p v b => jVariant "Let" (jObj [("pat", patJson p), ("value", exprJson v), ("body", exprJson b)])
    | .Comprehension var src flt body =>
      jVariant "Comprehension" (jObj [
        ("var", jStr var),
        ("source", exprJson src),
        ("filter", jOpt exprJson flt),
        ("body", jOpt exprJson body)
      ])
    | .Group src key flt fields =>
      jVariant "Group" (jObj [
        ("source", exprJson src),
        ("key", exprJson key),
        ("filter", jOpt exprJson flt),
        ("fields", jArr (fields.map fun (k, v) => jArr [jStr k, exprJson v]))
      ])
    | .Optimize per flt sense obj cs =>
      jVariant "Optimize" (jObj [
        ("per", jStr per),
        ("filter", jOpt exprJson flt),
        ("sense", senseJson sense),
        ("objective", exprJson obj),
        ("constraints", jArr (cs.map exprJson))
      ])
    | .Forall var dom body =>
      jVariant "Forall" (jObj [("var", jStr var), ("domain", domainJson dom), ("body", exprJson body)])
    | .Annotated e anns =>
      jVariant "Annotated" (jArr [exprJson e, jArr (anns.map annJson)])

  partial def argJson : Arg → String
    | .mk name val => jObj [("name", jOpt jStr name), ("value", exprJson val)]

  partial def annJson : Annotation → String
    | .mk name args pos => jObj [("name", jStr name), ("args", jArr (args.map argJson)), ("pos", posJson pos)]

  partial def armJson : Arm → String
    | .mk pat guard body => jObj [("pat", patJson pat), ("guard", jOpt exprJson guard), ("body", exprJson body)]

  partial def domainJson : Domain → String
    | .Type t => jVariant "Type" (tyJson t)
    | .In e => jVariant "In" (exprJson e)

  partial def tyJson : Ty → String
    | .Named name generics params pos =>
      jVariant "Named" (jObj [
        ("name", jStr name),
        ("generics", jArr (generics.map tyJson)),
        ("params", jArr (params.map tyParamJson)),
        ("pos", posJson pos)
      ])
    | .Union alts => jVariant "Union" (jArr (alts.map tyJson))

  partial def tyParamJson : TyParam → String
    | .Field name t => jVariant "Field" (jArr [jStr name, tyJson t])
    | .Expr e => jVariant "Expr" (exprJson e)

end

def fieldJson (f : Field) : String :=
  jObj [("name", jStr f.name), ("ty", tyJson f.ty), ("anns", jArr (f.anns.map annJson))]

def caseJson (c : Case) : String :=
  jObj [
    ("cond", exprJson c.cond),
    ("lets", jArr (c.lets.map fun (p, e) => jArr [patJson p, exprJson e])),
    ("outputs", jArr (c.outputs.map fun (k, e) => jArr [jStr k, exprJson e]))
  ]

def declKindJson : DeclKind → String
  | .Param ty default =>
    jVariant "Param" (jObj [("ty", tyJson ty), ("default", jOpt exprJson default)])
  | .TypeAlias ty =>
    jVariant "TypeAlias" (tyJson ty)
  | .Data base fields =>
    jVariant "Data" (jObj [("base", jOpt tyJson base), ("fields", jArr (fields.map fieldJson))])
  | .Event fields =>
    jVariant "Event" (jArr (fields.map fieldJson))
  | .View e =>
    jVariant "View" (exprJson e)
  | .Fn params ret body =>
    jVariant "Fn" (jObj [
      ("params", jArr (params.map fieldJson)),
      ("ret", tyJson ret),
      ("body", jOpt exprJson body)
    ])
  | .Alloc ty body =>
    jVariant "Alloc" (jObj [("ty", tyJson ty), ("body", exprJson body)])
  | .Partition var src targets split splitVar splitBy moreSplits cases =>
    jVariant "Partition" (jObj [
      ("var", jStr var),
      ("source", tyJson src),
      ("targets", jArr (targets.map jStr)),
      ("split", exprJson split),
      ("split_var", jStr splitVar),
      ("split_by", exprJson splitBy),
      ("more_splits", jArr (moreSplits.map fun (k, e) => jArr [jStr k, exprJson e])),
      ("cases", jArr (cases.map caseJson))
    ])
  | .Invariant e => jVariant "Invariant" (exprJson e)
  | .Theorem e => jVariant "Theorem" (exprJson e)

def declJson (d : Decl) : String :=
  jObj [
    ("name", jStr d.name),
    ("kind", declKindJson d.kind),
    ("anns", jArr (d.anns.map annJson)),
    ("pos", posJson d.pos)
  ]

def moduleJson (m : Module) : String :=
  jObj [
    ("name", jStr m.name),
    ("params", jArr (m.params.map argJson)),
    ("anns", jArr (m.anns.map annJson)),
    ("decls", jArr (m.decls.map declJson))
  ]

def cellDefJson : CellDef → String
  | .Input d => jVariant "Input" (jOpt exprJson d)
  | .Expr e => jVariant "Expr" (exprJson e)

def formItemJson (it : FormItem) : String :=
  jObj [
    ("id", jStr it.id),
    ("label", jStr it.label),
    ("observer", jBool it.observer),
    ("cells", jArr (it.cells.map fun (k, v) => jArr [jStr k, cellDefJson v])),
    ("anns", jArr (it.anns.map annJson)),
    ("pos", posJson it.pos)
  ]

def formCheckJson (c : FormCheck) : String :=
  jObj [
    ("warn", jBool c.warn),
    ("id", jStr c.id),
    ("description", jStr c.description),
    ("cond", exprJson c.cond),
    ("anns", jArr (c.anns.map annJson))
  ]

def formDeclJson (f : FormDecl) : String :=
  jObj [
    ("name", jStr f.name),
    ("code", jStr f.code),
    ("title", jStr f.title),
    ("columns", jArr (f.columns.map jStr)),
    ("input", jStr f.input),
    ("items", jArr (f.items.map formItemJson)),
    ("checks", jArr (f.checks.map formCheckJson)),
    ("theorems", jArr (f.theorems.map formCheckJson)),
    ("anns", jArr (f.anns.map annJson))
  ]

def topLevelJson : TopLevel → String
  | .Module m => jVariant "Module" (moduleJson m)
  | .Form f => jVariant "Form" (formDeclJson f)

def fileJson (f : File) : String :=
  jObj [("items", jArr (f.items.map topLevelJson))]

end Prudence.JSON
