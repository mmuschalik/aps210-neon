import Prudence.DSL
/-! Prudence Lean 4 form specification: ARF 210.2 (Minimum Liquidity Holdings Ratio) -/
set_option autoImplicit false
open Prudence.DSL

namespace Prudence.Specs.Forms.Arf210_2

def items_0 : List FormItem := [
  { id := "1", label := "Cash", observer := false, cells := [("amount", exprCell (((((cell "1.1" "amount" { line := 11, col := 14 }) + (cell "1.2" "amount" { line := 11, col := 29 }))) + (cell "1.3" "amount" { line := 11, col := 44 }))))], anns := [cite "sum of 1.1, 1.2, 1.3" { line := 12, col := 5 }], pos := { line := 10, col := 3 } },
  { id := "1.1", label := "Notes and coin", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 14, col := 3 } },
  { id := "1.2", label := "Settlement funds due from clearing houses", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 18, col := 3 } },
  { id := "1.3", label := "Settlement funds due from", observer := false, cells := [("amount", exprCell (((((((((cell "1.3.1" "amount" { line := 23, col := 14 }) + (cell "1.3.2" "amount" { line := 23, col := 31 }))) + (cell "1.3.3" "amount" { line := 23, col := 48 }))) + (cell "1.3.4" "amount" { line := 23, col := 65 }))) + (cell "1.3.5" "amount" { line := 23, col := 82 }))))], anns := [cite "sum of 1.3.1, 1.3.2, 1.3.3, 1.3.4, 1.3.5" { line := 24, col := 5 }], pos := { line := 22, col := 3 } },
  { id := "1.3.1", label := "RBA", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 26, col := 3 } },
  { id := "1.3.2", label := "Major banks", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 30, col := 3 } },
  { id := "1.3.3", label := "Other locally incorporated LCR ADIs", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 34, col := 3 } },
  { id := "1.3.4", label := "Other locally incorporated ADIs not included above", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 38, col := 3 } },
  { id := "1.3.5", label := "Foreign ADIs", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 42, col := 3 } },
  { id := "2", label := "Australian Government and semi-government securities", observer := false, cells := [("amount", exprCell (((cell "2.1" "amount" { line := 47, col := 14 }) + (cell "2.2" "amount" { line := 47, col := 29 }))))], anns := [cite "sum of 2.1, 2.2" { line := 48, col := 5 }], pos := { line := 46, col := 3 } },
  { id := "2.1", label := "Australian Government securities", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 50, col := 3 } },
  { id := "2.2", label := "State Government or Territory Central Borrowing Authorities securities", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 54, col := 3 } },
  { id := "3", label := "Debt securities with an Australian Government guarantee", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 58, col := 3 } },
  { id := "4", label := "Supranationals and foreign governments", observer := false, cells := [("amount", exprCell (((((cell "4.1" "amount" { line := 63, col := 14 }) + (cell "4.2" "amount" { line := 63, col := 29 }))) + (cell "4.3" "amount" { line := 63, col := 44 }))))], anns := [cite "sum of 4.1, 4.2, 4.3" { line := 64, col := 5 }], pos := { line := 62, col := 3 } },
  { id := "4.1", label := "Debt securities issued by supranationals", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 66, col := 3 } }
]

def items_1 : List FormItem := [
  { id := "4.2", label := "Debt securities issued by foreign sovereign governments", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 70, col := 3 } },
  { id := "4.3", label := "Debt securities guaranteed by foreign sovereign governments", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 74, col := 3 } },
  { id := "5", label := "Bank bills, certificates of deposit and debt securities issued by ADIs", observer := false, cells := [("amount", exprCell (((((cell "5.1" "amount" { line := 79, col := 14 }) + (cell "5.2" "amount" { line := 79, col := 29 }))) + (cell "5.3" "amount" { line := 79, col := 44 }))))], anns := [cite "sum of 5.1, 5.2, 5.3" { line := 80, col := 5 }], pos := { line := 78, col := 3 } },
  { id := "5.1", label := "Bank bills, commercial paper, other non-deposit obligations", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 82, col := 3 } },
  { id := "5.2", label := "Negotiable certificates of deposit", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 86, col := 3 } },
  { id := "5.3", label := "Debt securities", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 90, col := 3 } },
  { id := "6", label := "Deposits held with other ADIs", observer := false, cells := [("amount", exprCell (((cell "6.1" "amount" { line := 95, col := 14 }) + (cell "6.2" "amount" { line := 95, col := 29 }))))], anns := [cite "sum of 6.1, 6.2" { line := 96, col := 5 }], pos := { line := 94, col := 3 } },
  { id := "6.1", label := "Deposits on a call basis", observer := false, cells := [("amount", exprCell (((((((((cell "6.1.1" "amount" { line := 99, col := 14 }) + (cell "6.1.2" "amount" { line := 99, col := 31 }))) + (cell "6.1.3" "amount" { line := 99, col := 48 }))) + (cell "6.1.4" "amount" { line := 99, col := 65 }))) + (cell "6.1.5" "amount" { line := 99, col := 82 }))))], anns := [cite "sum of 6.1.1, 6.1.2, 6.1.3, 6.1.4, 6.1.5" { line := 100, col := 5 }], pos := { line := 98, col := 3 } },
  { id := "6.2", label := "Eligible deposits not on a call basis", observer := false, cells := [("amount", exprCell (((((((((cell "6.2.1" "amount" { line := 103, col := 14 }) + (cell "6.2.2" "amount" { line := 103, col := 31 }))) + (cell "6.2.3" "amount" { line := 103, col := 48 }))) + (cell "6.2.4" "amount" { line := 103, col := 65 }))) + (cell "6.2.5" "amount" { line := 103, col := 82 }))))], anns := [cite "sum of 6.2.1, 6.2.2, 6.2.3, 6.2.4, 6.2.5" { line := 104, col := 5 }], pos := { line := 102, col := 3 } },
  { id := "6.1.1", label := "Major banks", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 106, col := 3 } },
  { id := "6.1.2", label := "Other locally incorporated LCR ADIs", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 110, col := 3 } },
  { id := "6.1.3", label := "Other locally incorporated ADIs not included above", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 114, col := 3 } },
  { id := "6.1.4", label := "Foreign ADIs", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 118, col := 3 } },
  { id := "6.1.5", label := "Other deposits approved by APRA", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 122, col := 3 } },
  { id := "6.2.1", label := "Major banks", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 126, col := 3 } }
]

def items_2 : List FormItem := [
  { id := "6.2.2", label := "Other locally incorporated LCR ADIs", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 130, col := 3 } },
  { id := "6.2.3", label := "Other locally incorporated ADIs not included above", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 134, col := 3 } },
  { id := "6.2.4", label := "Foreign ADIs", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 138, col := 3 } },
  { id := "6.2.5", label := "Other deposits approved by APRA", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 142, col := 3 } },
  { id := "7", label := "Other securities approved by APRA", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 146, col := 3 } },
  { id := "8", label := "Total MLH", observer := false, cells := [("amount", exprCell (((((((((((((cell "1" "amount" { line := 151, col := 14 }) + (cell "2" "amount" { line := 151, col := 27 }))) + (cell "3" "amount" { line := 151, col := 40 }))) + (cell "4" "amount" { line := 151, col := 53 }))) + (cell "5" "amount" { line := 151, col := 66 }))) + (cell "6" "amount" { line := 151, col := 79 }))) + (cell "7" "amount" { line := 151, col := 92 }))))], anns := [cite "sum of 1, 2, 3, 4, 5, 6, 7" { line := 152, col := 5 }], pos := { line := 150, col := 3 } },
  { id := "8.1", label := "of which: deposits placed with ADIs with a credit rating grade 3 or lower", observer := true, cells := [("amount", inputCell)], anns := [], pos := { line := 154, col := 3 } },
  { id := "8.2", label := "of which: debt securities issued by ADIs with a credit rating grade 3 or lower", observer := true, cells := [("amount", inputCell)], anns := [], pos := { line := 158, col := 3 } },
  { id := "9", label := "Placements by other ADIs", observer := false, cells := [("amount", exprCell (((cell "9.1" "amount" { line := 163, col := 14 }) + (cell "9.2" "amount" { line := 163, col := 29 }))))], anns := [cite "sum of 9.1, 9.2" { line := 164, col := 5 }], pos := { line := 162, col := 3 } },
  { id := "9.1", label := "Deposits on a call basis placed with the reporting ADI by other ADIs", observer := false, cells := [("amount", exprCell (((((((cell "9.1.1" "amount" { line := 167, col := 14 }) + (cell "9.1.2" "amount" { line := 167, col := 31 }))) + (cell "9.1.3" "amount" { line := 167, col := 48 }))) + (cell "9.1.4" "amount" { line := 167, col := 65 }))))], anns := [cite "sum of 9.1.1, 9.1.2, 9.1.3, 9.1.4" { line := 168, col := 5 }], pos := { line := 166, col := 3 } },
  { id := "9.2", label := "Deposits not on a call basis placed with the reporting ADI by other ADIs", observer := false, cells := [("amount", exprCell (((((((cell "9.2.1" "amount" { line := 171, col := 14 }) + (cell "9.2.2" "amount" { line := 171, col := 31 }))) + (cell "9.2.3" "amount" { line := 171, col := 48 }))) + (cell "9.2.4" "amount" { line := 171, col := 65 }))))], anns := [cite "sum of 9.2.1, 9.2.2, 9.2.3, 9.2.4" { line := 172, col := 5 }], pos := { line := 170, col := 3 } },
  { id := "9.1.1", label := "Major banks", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 174, col := 3 } },
  { id := "9.1.2", label := "Other locally incorporated LCR ADIs", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 178, col := 3 } },
  { id := "9.1.3", label := "Other locally incorporated ADIs not included above", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 182, col := 3 } },
  { id := "9.1.4", label := "Foreign ADIs", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 186, col := 3 } }
]

def items_3 : List FormItem := [
  { id := "9.2.1", label := "Major banks", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 190, col := 3 } },
  { id := "9.2.2", label := "Other locally incorporated LCR ADIs", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 194, col := 3 } },
  { id := "9.2.3", label := "Other locally incorporated ADIs not included above", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 198, col := 3 } },
  { id := "9.2.4", label := "Foreign ADIs", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 202, col := 3 } },
  { id := "10", label := "Placements by other ADIs in excess of MLH deposits of the reporting ADI", observer := false, cells := [("amount", exprCell (call (id "max" { line := 207, col := 14 }) [arg (((cell "9" "amount" { line := 207, col := 18 }) - (cell "6" "amount" { line := 207, col := 31 }))), arg (num "0" { line := 207, col := 43 })]))], anns := [cite "ARS p.71 formula; netting across ADIs, not bilateral" { line := 208, col := 5 }], pos := { line := 206, col := 3 } },
  { id := "11", label := "Total adjusted MLH", observer := false, cells := [("amount", exprCell (((((cell "8" "amount" { line := 211, col := 14 }) - (cell "9" "amount" { line := 211, col := 27 }))) + (cell "10" "amount" { line := 211, col := 40 }))))], anns := [cite "ARS p.71 formula" { line := 212, col := 5 }], pos := { line := 210, col := 3 } },
  { id := "12", label := "Liability base", observer := false, cells := [("amount", exprCell (((((cell "12.1" "amount" { line := 215, col := 14 }) - (cell "12.2" "amount" { line := 215, col := 30 }))) - (cell "12.3" "amount" { line := 215, col := 46 }))))], anns := [cite "ARS p.72 formula" { line := 216, col := 5 }], pos := { line := 214, col := 3 } },
  { id := "12.1", label := "Total liabilities", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 218, col := 3 } },
  { id := "12.2", label := "Liabilities of securitisation SPVs meeting APS 120 capital relief requirements", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 222, col := 3 } },
  { id := "12.3", label := "Placements by other ADIs up to the amount of MLH deposits of the reporting ADI", observer := false, cells := [("amount", exprCell (((cell "9" "amount" { line := 227, col := 14 }) - (cell "10" "amount" { line := 227, col := 27 }))))], anns := [cite "ARS p.72 formula" { line := 228, col := 5 }], pos := { line := 226, col := 3 } },
  { id := "13", label := "Off-balance sheet irrevocable commitments", observer := false, cells := [("amount", exprCell (((((((((((((((cell "13.1" "amount" { line := 231, col := 14 }) + (cell "13.2" "amount" { line := 231, col := 30 }))) + (cell "13.3" "amount" { line := 231, col := 46 }))) + (cell "13.4" "amount" { line := 231, col := 62 }))) + (cell "13.5" "amount" { line := 231, col := 78 }))) + (cell "13.6" "amount" { line := 231, col := 94 }))) + (cell "13.7" "amount" { line := 231, col := 110 }))) + (cell "13.8" "amount" { line := 231, col := 126 }))))], anns := [cite "sum of 13.1, 13.2, 13.3, 13.4, 13.5, 13.6, 13.7, 13.8" { line := 232, col := 5 }], pos := { line := 230, col := 3 } },
  { id := "13.1", label := "Trade finance related obligations", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 234, col := 3 } },
  { id := "13.2", label := "Guarantees", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 238, col := 3 } },
  { id := "13.3", label := "Standby letters of credit", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 242, col := 3 } },
  { id := "13.4", label := "Undrawn formal standby facilities", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 246, col := 3 } }
]

def items_4 : List FormItem := [
  { id := "13.5", label := "Loans approved but not advanced/funded", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 250, col := 3 } },
  { id := "13.6", label := "Balance available for redraw of term loans", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 254, col := 3 } },
  { id := "13.7", label := "Balance available under overdraft agreements", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 258, col := 3 } },
  { id := "13.8", label := "All other irrevocable commitments", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 262, col := 3 } },
  { id := "14", label := "Adjusted liability base", observer := false, cells := [("amount", exprCell (((cell "12" "amount" { line := 267, col := 14 }) + (cell "13" "amount" { line := 267, col := 28 }))))], anns := [cite "sum of 12, 13" { line := 268, col := 5 }], pos := { line := 266, col := 3 } },
  { id := "15", label := "MLH ratio (%)", observer := false, cells := [("amount", exprCell (((((cell "11" "amount" { line := 271, col := 14 }) / (cell "14" "amount" { line := 271, col := 28 }))) * (num "100" { line := 271, col := 42 }))))], anns := [cite "ARS p.73" { line := 272, col := 5 }], pos := { line := 270, col := 3 } },
  { id := "16", label := "Minimum MLH ratio per liquidity management strategy", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 274, col := 3 } },
  { id := "17", label := "Lowest MLH ratio during reporting period", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 278, col := 3 } },
  { id := "18", label := "Average MLH ratio during reporting period", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 282, col := 3 } },
  { id := "19", label := "Highest MLH ratio during reporting period", observer := false, cells := [("amount", inputCell)], anns := [], pos := { line := 286, col := 3 } }
]

def items : List FormItem := items_0 ++ items_1 ++ items_2 ++ items_3 ++ items_4

def checks : List FormCheck := [
  { warn := false, id := "8.1<=6", description := "Low-rated ADI deposits exceed item 6", cond := ((cell "8.1" "amount" { line := 290, col := 58 }) <=. (cell "6" "amount" { line := 290, col := 74 })), anns := [] },
  { warn := false, id := "8.2<=5+7", description := "Low-rated ADI securities exceed items 5 and 7", cond := ((cell "8.2" "amount" { line := 291, col := 69 }) <=. (((cell "5" "amount" { line := 291, col := 85 }) + (cell "7" "amount" { line := 291, col := 98 })))), anns := [] }
]

def theorems : List FormCheck := [
  { warn := false, id := "netting-never-adds", description := "Netting placements by other ADIs never increases holdings or the liability base", cond := ((((((cell "6" "amount" { line := 294, col := 5 }) >=. (num "0" { line := 294, col := 19 }))) &&. (((cell "9" "amount" { line := 294, col := 24 }) >=. (num "0" { line := 294, col := 38 }))))) ==> (((((((cell "11" "amount" { line := 295, col := 9 }) <=. (cell "8" "amount" { line := 295, col := 24 }))) &&. (((num "0" { line := 295, col := 38 }) <=. (cell "12.3" "amount" { line := 295, col := 43 }))))) &&. (((cell "12.3" "amount" { line := 295, col := 60 }) <=. (cell "9" "amount" { line := 295, col := 77 })))))), anns := [cite "ARS 210.0 p.71" { line := 295, col := 88 }] }
]

def spec : FormDecl := {
  name     := "Arf210_2"
  code     := "ARF 210.2"
  title    := "Minimum Liquidity Holdings Ratio"
  columns  := ["amount"]
  input    := "amount"
  items    := items
  checks   := checks
  theorems := theorems
  anns     := [ann "reporting" [arg (str "ARS 210.0" { line := 5, col := 12 }), kwArg "version" (str "2023-03-31" { line := 5, col := 34 })] { line := 5, col := 1 }]
}

end Prudence.Specs.Forms.Arf210_2
