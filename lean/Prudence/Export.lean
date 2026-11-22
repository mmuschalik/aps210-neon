import Prudence.DSL
import Prudence.JSON
import Prudence.SQL
import Prudence.Canonical
import Prudence.Specs.Forms.Arf210_1A
import Prudence.Specs.Forms.Arf210_1B
import Prudence.Specs.Forms.Arf210_2
import Prudence.Specs.Forms.Arf210_3_1
import Prudence.Specs.Forms.Arf210_3_2
import Prudence.Specs.Forms.Arf210_4
import Prudence.Specs.Forms.Arf210_5
import Prudence.Specs.Forms.Arf210_6
import Prudence.Specs.Modules.RetailSme
import Prudence.Specs.Modules.Wholesale
import Prudence.Specs.Modules.SecuredFunding
import Prudence.Specs.Modules.Loans
import Prudence.Specs.Modules.Derivatives
import Prudence.Specs.Modules.Funding
import Prudence.Specs.Modules.Commitments

/-!
# Prudence Lean 4 Exporter (`lake exe prudence-export`)

Exports all 8 ARF 210 form specs and all 7 rule module specs from the
idiomatic Lean 4 DSL into:
1. Prudence IR JSON (`specs/ir/forms/*.json`, `specs/ir/modules/*.json`)
   for `prudence-core` (Z3 proofs, exact-rational interpreter, LP solver, checker, trace);
2. DuckDB SQL (`sql/forms/*.sql`, `sql/modules/*.sql`).
-/

open Prudence.DSL
open Prudence.JSON
open Prudence.SQL

def allForms : List (String × FormDecl) := [
  ("arf_210_1a",  Prudence.Specs.Forms.Arf210_1A.spec),
  ("arf_210_1b",  Prudence.Specs.Forms.Arf210_1B.spec),
  ("arf_210_2",   Prudence.Specs.Forms.Arf210_2.spec),
  ("arf_210_3_1", Prudence.Specs.Forms.Arf210_3_1.spec),
  ("arf_210_3_2", Prudence.Specs.Forms.Arf210_3_2.spec),
  ("arf_210_4",   Prudence.Specs.Forms.Arf210_4.spec),
  ("arf_210_5",   Prudence.Specs.Forms.Arf210_5.spec),
  ("arf_210_6",   Prudence.Specs.Forms.Arf210_6.spec)
]

def allModules : List (String × Module) := [
  ("retail_sme",      Prudence.Specs.Modules.RetailSme.spec),
  ("wholesale",       Prudence.Specs.Modules.Wholesale.spec),
  ("secured_funding", Prudence.Specs.Modules.SecuredFunding.spec),
  ("loans",           Prudence.Specs.Modules.Loans.spec),
  ("derivatives",     Prudence.Specs.Modules.Derivatives.spec),
  ("funding",         Prudence.Specs.Modules.Funding.spec),
  ("commitments",     Prudence.Specs.Modules.Commitments.spec)
]

def main (args : List String) : IO Unit := do
  let root : System.FilePath := match args.head? with
    | some dir => dir
    | none     => ".."
  let irFormsDir  := root / "specs" / "ir" / "forms"
  let irModsDir   := root / "specs" / "ir" / "modules"
  let sqlFormsDir := root / "sql" / "forms"
  let sqlModsDir  := root / "sql" / "modules"
  IO.FS.createDirAll irFormsDir
  IO.FS.createDirAll irModsDir
  IO.FS.createDirAll sqlFormsDir
  IO.FS.createDirAll sqlModsDir

  let canonPath := root / "sql" / "canonical_counterparty.sql"
  IO.FS.writeFile canonPath Prudence.Canonical.canonicalCounterpartySql
  IO.println s!"exported canonical counterparty -> {canonPath}"

  for (stem, f) in allForms do
    let irPath  := irFormsDir / s!"{stem}.json"
    let sqlPath := sqlFormsDir / s!"{stem}.sql"
    IO.FS.writeFile irPath  (fileJson (formFile f) ++ "\n")
    IO.FS.writeFile sqlPath (compileForm f)
    IO.println s!"exported form {f.code} -> {irPath} & {sqlPath}"

  for (stem, m) in allModules do
    let irPath  := irModsDir / s!"{stem}.json"
    let sqlPath := sqlModsDir / s!"{stem}.sql"
    IO.FS.writeFile irPath  (fileJson (moduleFile m) ++ "\n")
    IO.FS.writeFile sqlPath (compileModule m)
    IO.println s!"exported module {m.name} -> {irPath} & {sqlPath}"
