/-!
# Prudence Idiomatic Lean 4 DSL & Core IR

Source-of-truth specification language embedded in idiomatic Lean 4.
Every `FormDecl` and `Module` defined with this DSL:
1. Type-checks in Lean 4;
2. Exports to Prudence JSON IR (`Prudence.JSON`) for Z3 verification, exact-rational
   evaluation, LP solving, and the trusted checker (`prudence-core`);
3. Compiles directly to DuckDB SQL (`Prudence.SQL`) for analytical/warehouse execution.
-/

set_option autoImplicit false

namespace Prudence.DSL

structure Pos where
  line : Nat := 0
  col  : Nat := 0
  deriving Repr, BEq, Inhabited

inductive BinOp where
  | Implies | Or | And
  | Eq | Ne | Lt | Le | Gt | Ge | In
  | Add | Sub | Mul | Div
  deriving Repr, BEq, Inhabited

inductive UnOp where
  | Not | Neg
  deriving Repr, BEq, Inhabited

inductive Sense where
  | Maximize | Minimize
  deriving Repr, BEq, Inhabited

inductive Pat where
  | Wild
  | Bind (name : String)
  | Ctor (name : String) (args : List Pat)
  | Tuple (items : List Pat)
  deriving Repr, BEq, Inhabited

mutual

  inductive Expr where
    | Num (val : String) (pos : Pos)
    | Percent (val : String) (pos : Pos)
    | Str (val : String) (pos : Pos)
    | Ident (name : String) (pos : Pos)
    | Cell (item : String) (col : String) (pos : Pos)
    | List (items : List Expr)
    | Tuple (items : List Expr)
    | Unary (op : UnOp) (arg : Expr)
    | Binary (op : BinOp) (lhs : Expr) (rhs : Expr)
    | Call (callee : Expr) (args : List Arg)
    | Index (target : Expr) (idx : Expr)
    | Member (target : Expr) (field : String)
    | Is (target : Expr) (ctor : String)
    | If (cond : Expr) (then_ : Expr) (other : Expr)
    | IfLet (pat : Pat) (value : Expr) (then_ : Expr) (other : Expr)
    | Match (scrutinee : Expr) (arms : List Arm)
    | Let (pat : Pat) (value : Expr) (body : Expr)
    | Comprehension (var : String) (source : Expr) (filter : Option Expr) (body : Option Expr)
    | Group (source : Expr) (key : Expr) (filter : Option Expr) (fields : List (String × Expr))
    | Optimize (per : String) (filter : Option Expr) (sense : Sense) (objective : Expr) (constraints : List Expr)
    | Forall (var : String) (domain : Domain) (body : Expr)
    | Annotated (expr : Expr) (anns : List Annotation)
    deriving Repr, Inhabited

  inductive Arg where
    | mk (name : Option String) (value : Expr)
    deriving Repr, Inhabited

  inductive Annotation where
    | mk (name : String) (args : List Arg) (pos : Pos)
    deriving Repr, Inhabited

  inductive Arm where
    | mk (pat : Pat) (guard : Option Expr) (body : Expr)
    deriving Repr, Inhabited

  inductive Domain where
    | Type (ty : Ty)
    | In (source : Expr)
    deriving Repr, Inhabited

  inductive Ty where
    | Named (name : String) (generics : List Ty) (params : List TyParam) (pos : Pos)
    | Union (alts : List Ty)
    deriving Repr, Inhabited

  inductive TyParam where
    | Field (name : String) (ty : Ty)
    | Expr (expr : Expr)
    deriving Repr, Inhabited

end

structure Field where
  name : String
  ty   : Ty
  anns : List Annotation := []
  deriving Repr, Inhabited

structure Case where
  cond    : Expr
  lets    : List (Pat × Expr) := []
  outputs : List (String × Expr) := []
  deriving Repr, Inhabited

inductive DeclKind where
  | Param (ty : Ty) (default : Option Expr := none)
  | TypeAlias (ty : Ty)
  | Data (base : Option Ty := none) (fields : List Field := [])
  | Event (fields : List Field)
  | View (expr : Expr)
  | Fn (params : List Field) (ret : Ty) (body : Option Expr)
  | Alloc (ty : Ty) (body : Expr)
  | Partition
      (var : String)
      (source : Ty)
      (targets : List String)
      (split : Expr)
      (splitVar : String)
      (splitBy : Expr)
      (moreSplits : List (String × Expr) := [])
      (cases : List Case := [])
  | Invariant (expr : Expr)
  | Theorem (expr : Expr)
  deriving Repr, Inhabited

structure Decl where
  name : String
  kind : DeclKind
  anns : List Annotation := []
  pos  : Pos := {}
  deriving Repr, Inhabited

structure Module where
  name   : String
  params : List Arg := []
  anns   : List Annotation := []
  decls  : List Decl := []
  deriving Repr, Inhabited

inductive CellDef where
  | Input (default : Option Expr := none)
  | Expr (expr : Expr)
  deriving Repr, Inhabited

structure FormItem where
  id       : String
  label    : String
  observer : Bool := false
  cells    : List (String × CellDef) := []
  anns     : List Annotation := []
  pos      : Pos := {}
  deriving Repr, Inhabited

structure FormCheck where
  warn        : Bool := false
  id          : String
  description : String
  cond        : Expr
  anns        : List Annotation := []
  deriving Repr, Inhabited

structure FormDecl where
  name     : String
  code     : String
  title    : String
  columns  : List String
  input    : String
  items    : List FormItem
  checks   : List FormCheck := []
  theorems : List FormCheck := []
  anns     : List Annotation := []
  deriving Repr, Inhabited

inductive TopLevel where
  | Module (m : Module)
  | Form (f : FormDecl)
  deriving Repr, Inhabited

structure File where
  items : List TopLevel
  deriving Repr, Inhabited

-- ============================================================================
-- Ergonomic DSL combinators & operators
-- ============================================================================

instance : Add Expr := ⟨fun a b => .Binary .Add a b⟩
instance : Sub Expr := ⟨fun a b => .Binary .Sub a b⟩
instance : Mul Expr := ⟨fun a b => .Binary .Mul a b⟩
instance : Div Expr := ⟨fun a b => .Binary .Div a b⟩
instance : Neg Expr := ⟨fun a => .Unary .Neg a⟩

-- Expressions
abbrev num (s : String) (pos : Pos := {}) : Expr := .Num s pos
abbrev pct (s : String) (pos : Pos := {}) : Expr := .Percent s pos
abbrev str (s : String) (pos : Pos := {}) : Expr := .Str s pos
abbrev id (s : String) (pos : Pos := {}) : Expr := .Ident s pos
abbrev cell (i c : String) (pos : Pos := {}) : Expr := .Cell i c pos
abbrev list (xs : List Expr) : Expr := .List xs
abbrev tup (xs : List Expr) : Expr := .Tuple xs
abbrev not_ (e : Expr) : Expr := .Unary .Not e
abbrev neg_ (e : Expr) : Expr := .Unary .Neg e
abbrev bin (op : BinOp) (a b : Expr) : Expr := .Binary op a b
abbrev call (f : Expr) (args : List Arg) : Expr := .Call f args
abbrev fnCall (name : String) (args : List Arg) : Expr := .Call (.Ident name {}) args
abbrev idx (t i : Expr) : Expr := .Index t i
abbrev dot (t : Expr) (f : String) : Expr := .Member t f
abbrev isA (t : Expr) (c : String) : Expr := .Is t c
abbrev ite_ (c t e : Expr) : Expr := .If c t e
abbrev ifLet (p : Pat) (v t e : Expr) : Expr := .IfLet p v t e
abbrev match_ (s : Expr) (arms : List Arm) : Expr := .Match s arms
abbrev let_ (p : Pat) (v b : Expr) : Expr := .Let p v b
abbrev comp (var : String) (src : Expr) (flt : Option Expr := none) (body : Option Expr := none) : Expr :=
  .Comprehension var src flt body
abbrev group_ (src key : Expr) (flt : Option Expr := none) (fields : List (String × Expr) := []) : Expr :=
  .Group src key flt fields
abbrev opt (per : String) (flt : Option Expr) (sense : Sense) (obj : Expr) (cs : List Expr) : Expr :=
  .Optimize per flt sense obj cs
abbrev forall_ (var : String) (dom : Domain) (body : Expr) : Expr := .Forall var dom body
abbrev annotated (e : Expr) (anns : List Annotation) : Expr := .Annotated e anns

-- Comparison / logical operators
infixr:25 " ==> " => fun a b => Expr.Binary BinOp.Implies a b
infixr:30 " ||. " => fun a b => Expr.Binary BinOp.Or a b
infixr:35 " &&. " => fun a b => Expr.Binary BinOp.And a b
infix:50 " ==. "  => fun a b => Expr.Binary BinOp.Eq a b
infix:50 " !=. "  => fun a b => Expr.Binary BinOp.Ne a b
infix:50 " <. "   => fun a b => Expr.Binary BinOp.Lt a b
infix:50 " <=. "  => fun a b => Expr.Binary BinOp.Le a b
infix:50 " >. "   => fun a b => Expr.Binary BinOp.Gt a b
infix:50 " >=. "  => fun a b => Expr.Binary BinOp.Ge a b
infix:50 " in. "  => fun a b => Expr.Binary BinOp.In a b

-- Arguments & Annotations
abbrev arg (v : Expr) : Arg := .mk none v
abbrev kwArg (k : String) (v : Expr) : Arg := .mk (some k) v
abbrev ann (name : String) (args : List Arg := []) (pos : Pos := {}) : Annotation := .mk name args pos
abbrev cite (s : String) (pos : Pos := {}) : Annotation := .mk "cite" [.mk none (.Str s pos)] pos

-- Arms & Patterns
abbrev arm (p : Pat) (body : Expr) (guard : Option Expr := none) : Arm := .mk p guard body
abbrev pWild : Pat := .Wild
abbrev pBind (s : String) : Pat := .Bind s
abbrev pCtor (s : String) (ps : List Pat := []) : Pat := .Ctor s ps
abbrev pTup (ps : List Pat) : Pat := .Tuple ps

-- Types
abbrev ty (name : String) (generics : List Ty := []) (params : List TyParam := []) (pos : Pos := {}) : Ty :=
  .Named name generics params pos
abbrev unionTy (alts : List Ty) : Ty := .Union alts
abbrev tyField (name : String) (t : Ty) : TyParam := .Field name t
abbrev tyExpr (e : Expr) : TyParam := .Expr e

-- Forms
abbrev inputCell (default : Option Expr := none) : CellDef := .Input default
abbrev exprCell (e : Expr) : CellDef := .Expr e

def formFile (f : FormDecl) : File := { items := [.Form f] }
def moduleFile (m : Module) : File := { items := [.Module m] }

end Prudence.DSL
