-- Z3-synthesized branch-covering witness dataset for module `Loans`
-- Every row below was solved by Z3 to satisfy a distinct leaf branch + domain constraints.

INSERT INTO loans_params (mode, reporting_date) VALUES ('Conservative', '2026-09-30');

-- Branch #1: outstanding/LAM => max(..)
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('outstanding_1', 'OtherEntity', 'IntraGroupBank', 'OtherLoan', 1, 0, '2025-09-30', 12, NULL, NULL, 100, NULL, 'LAM', 1, 1, '2025-09-30', 0);

-- Branch #2: outstanding/ANN/then => max(..)
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('outstanding_2', 'OtherEntity', 'IntraGroupBank', 'OtherLoan', 1, 0, '2025-09-30', 1, NULL, NULL, 0, NULL, 'ANN', 1, 1, '2025-09-30', 0);

-- Branch #3: outstanding/ANN/else => max(..)
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('outstanding_3', 'OtherEntity', 'IntraGroupBank', 'OtherLoan', 1, 1, '2025-09-30', 12, NULL, NULL, 0, NULL, 'ANN', 1, 1, '2025-09-30', 0);

-- Branch #4: outstanding/PAM/then => <expr>
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('outstanding_4', 'OtherEntity', 'IntraGroupBank', 'OtherLoan', 1, 0, '2025-09-30', 1, NULL, NULL, 0, NULL, 'PAM', 1, 1, '2025-09-30', 0);

-- Branch #5: outstanding/PAM/else => <expr>
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('outstanding_5', 'OtherEntity', 'IntraGroupBank', 'OtherLoan', 1, 0, '2025-09-30', 1, NULL, NULL, 0, NULL, 'PAM', 1, 1, '2025-10-01', 0);

-- Branch #6: outstanding/UMP => <expr>
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('outstanding_6', 'OtherEntity', 'IntraGroupBank', 'OtherLoan', 1, 0, '2025-09-30', 12, NULL, NULL, 100, NULL, 'UMP', 1, 1, '2025-09-30', 0);

-- Branch #7: cash/UMP/then => min(..)
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('cash_7', 'OtherEntity', 'IntraGroupBank', 'OtherLoan', 1, 0, '2025-09-30', 12, NULL, NULL, 100, NULL, 'UMP', 1, 1, '2025-09-30', 0);

-- Branch #8: cash/UMP/else => <expr>
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('cash_8', 'OtherEntity', 'IntraGroupBank', 'OtherLoan', 1, 0, '2025-09-30', 6, NULL, NULL, 100, NULL, 'UMP', 1, 1, '2025-09-30', 0);

-- Branch #9: cash/otherwise => <expr>
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('cash_9', 'OtherEntity', 'IntraGroupBank', 'OtherLoan', 1, 0, '2025-09-30', 1, NULL, NULL, 0, NULL, 'PAM', 1, 1, '2025-09-30', 0);

-- Branch #10: last_payment_day/PAM => maturity
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('last_payment_day_10', 'OtherEntity', 'IntraGroupBank', 'OtherLoan', 1, 0, '2025-09-30', 12, NULL, NULL, 100, NULL, 'PAM', 1, 1, '2025-09-30', 0);

-- Branch #11: last_payment_day/otherwise => <expr>
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('last_payment_day_11', 'OtherEntity', 'IntraGroupBank', 'OtherLoan', 1, 0, '2025-09-30', 1, NULL, NULL, 20, NULL, 'ANN', 1, 1, '2025-09-30', 0);

-- Branch #12: inflow_30d/then => <expr>
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('inflow_30d_12', 'OtherEntity', 'IntraGroupBank', 'OtherLoan', 1, 0, '2025-09-30', 1, NULL, NULL, 0, NULL, 'PAM', 1, 1, '2025-09-30', 0);

-- Branch #13: inflow_30d/else => sum(..)
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('inflow_30d_13', 'OtherEntity', 'IntraGroupBank', 'OtherLoan', 1, 0, '2025-09-30', 1, TRUE, NULL, 0, NULL, 'PAM', 1, 1, '2025-09-30', 0);

-- Branch #14: inflow_row/RetailCustomer => "20.1"
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('inflow_row_14', 'RetailCustomer', 'IntraGroupBank', 'OtherLoan', 1, 0, '2025-09-30', 12, NULL, NULL, 100, NULL, 'PAM', 1, 1, '2025-09-30', 0);

-- Branch #15: inflow_row/Sme => "20.2"
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('inflow_row_15', 'Sme', 'IntraGroupBank', 'OtherLoan', 1, 0, '2025-09-30', 12, NULL, NULL, 100, NULL, 'PAM', 1, 1, '2025-09-30', 0);

-- Branch #16: inflow_row/NonFinancialCorporate => "20.3"
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('inflow_row_16', 'NonFinancialCorporate', 'IntraGroupBank', 'OtherLoan', 1, 0, '2025-09-30', 12, NULL, NULL, 100, NULL, 'PAM', 1, 1, '2025-09-30', 0);

-- Branch #17: inflow_row/CentralBank => "20.4"
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('inflow_row_17', 'CentralBank', 'IntraGroupBank', 'OtherLoan', 1, 0, '2025-09-30', 12, NULL, NULL, 100, NULL, 'PAM', 1, 1, '2025-09-30', 0);

-- Branch #18: inflow_row/FinancialInstitution => "20.5"
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('inflow_row_18', 'FinancialInstitution', 'IntraGroupBank', 'OtherLoan', 1, 0, '2025-09-30', 12, NULL, NULL, 100, NULL, 'PAM', 1, 1, '2025-09-30', 0);

-- Branch #19: inflow_row/OtherEntity => "20.6"
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('inflow_row_19', 'OtherEntity', 'IntraGroupBank', 'OtherLoan', 1, 0, '2025-09-30', 12, NULL, NULL, 100, NULL, 'PAM', 1, 1, '2025-09-30', 0);

-- Branch #20: intra_group_row/External => "none"
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('intra_group_row_20', 'OtherEntity', 'External', 'OtherLoan', 1, 0, '2025-09-30', 12, NULL, NULL, 100, NULL, 'PAM', 1, 1, '2025-09-30', 0);

-- Branch #21: intra_group_row/IntraGroupBank => "20.7"
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('intra_group_row_21', 'OtherEntity', 'IntraGroupBank', 'OtherLoan', 1, 0, '2025-09-30', 12, NULL, NULL, 100, NULL, 'PAM', 1, 1, '2025-09-30', 0);

-- Branch #22: intra_group_row/IntraGroupFinancial => "20.8"
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('intra_group_row_22', 'OtherEntity', 'IntraGroupFinancial', 'OtherLoan', 1, 0, '2025-09-30', 12, NULL, NULL, 100, NULL, 'PAM', 1, 1, '2025-09-30', 0);

-- Branch #23: intra_group_row/IntraGroupOther => "20.9"
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('intra_group_row_23', 'OtherEntity', 'IntraGroupOther', 'OtherLoan', 1, 0, '2025-09-30', 12, NULL, NULL, 100, NULL, 'PAM', 1, 1, '2025-09-30', 0);

-- Branch #24: inflow/then => inflow_30d(..)
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('inflow_24', 'RetailCustomer', 'External', 'OtherLoan', 1, 0, '2025-09-30', 1, NULL, NULL, 100, NULL, 'PAM', 1, 1, '2025-09-30', 0);

-- Branch #25: inflow/else => <expr>
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('inflow_25', 'RetailCustomer', 'External', 'OtherLoan', 1, 0, '2025-09-30', 1, NULL, NULL, 100, NULL, 'PAM', 1, 1, '2025-09-30', 0);

-- Branch #26: other_inflow/then => <expr>
INSERT INTO receivable (id, amount, due, non_financial_revenue, contingent, performing) VALUES ('other_inflow_26', 1, '2026-10-01', FALSE, FALSE, TRUE);

-- Branch #27: other_inflow/else => <expr>
INSERT INTO receivable (id, amount, due, non_financial_revenue, contingent, performing) VALUES ('other_inflow_27', 1, '2025-09-30', TRUE, TRUE, NULL);

-- Branch #28: principal_before/UMP => <expr>
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('principal_before_28', 'OtherEntity', 'IntraGroupBank', 'OtherLoan', 1, 0, '2025-09-30', 12, NULL, NULL, 100, NULL, 'UMP', 1, 1, '2025-09-30', 0);

-- Branch #29: principal_before/otherwise => max(..)
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('principal_before_29', 'OtherEntity', 'IntraGroupBank', 'OtherLoan', 1, 0, '2025-09-30', 1, NULL, NULL, 0, NULL, 'PAM', 1, 1, '2025-09-30', 0);

-- Branch #30: rsf_item/then => "39"
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('rsf_item_30', 'OtherEntity', 'IntraGroupBank', 'OtherLoan', 1, 0, '2025-09-30', 1, NULL, NULL, 0, NULL, 'PAM', 1, 1, '2025-09-30', 0);

-- Branch #31: rsf_item/else/then => "24"
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('rsf_item_31', 'FinancialInstitution', 'IntraGroupBank', 'OtherLoan', 1, 0, '2025-09-30', 1, TRUE, NULL, 20, NULL, 'PAM', 1, 1, '2025-09-30', 0);

-- Branch #32: rsf_item/else/else/then => "35"
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('rsf_item_32', 'OtherEntity', 'IntraGroupBank', 'ResidentialMortgage', 1, 0, '2025-09-30', 1, TRUE, TRUE, 0, NULL, 'PAM', 1, 1, '2025-09-30', 0);

-- Branch #33: rsf_item/else/else/else/then => "36"
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('rsf_item_33', 'OtherEntity', 'IntraGroupBank', 'OtherLoan', 1, 0, '2025-09-30', 1, TRUE, NULL, 0, NULL, 'PAM', 1, 1, '2025-09-30', 0);

-- Branch #34: rsf_item/else/else/else/else => "37"
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('rsf_item_34', 'OtherEntity', 'IntraGroupBank', 'OtherLoan', 1, 0, '2025-09-30', 1, TRUE, NULL, 50, NULL, 'PAM', 1, 1, '2025-09-30', 0);

-- Branch #35: sub_item/then => "39"
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('sub_item_35', 'OtherEntity', 'IntraGroupBank', 'OtherLoan', 1, 0, '2025-09-30', 1, NULL, NULL, 0, NULL, 'PAM', 1, 1, '2025-09-30', 0);

-- Branch #36: sub_item/else/then/then => "24.2.3"
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('sub_item_36', 'FinancialInstitution', 'IntraGroupBank', 'OtherLoan', 1, 0, '2025-09-30', 1, TRUE, NULL, 20, '2027-09-25', 'PAM', 1, 1, '2025-09-30', 0);

-- Branch #37: sub_item/else/then/else/then => "24.2.2"
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('sub_item_37', 'FinancialInstitution', 'IntraGroupBank', 'OtherLoan', 1, 0, '2025-09-30', 1, TRUE, NULL, 20, '2027-03-29', 'PAM', 1, 1, '2025-09-30', 0);

-- Branch #38: sub_item/else/then/else/else/then => "24.2.1"
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('sub_item_38', 'FinancialInstitution', 'IntraGroupBank', 'OtherLoan', 1, 0, '2025-09-30', 1, TRUE, NULL, 20, '2026-10-01', 'PAM', 1, 1, '2025-09-30', 0);

-- Branch #39: sub_item/else/then/else/else/else => "24.1"
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('sub_item_39', 'FinancialInstitution', 'IntraGroupBank', 'OtherLoan', 1, 0, '2025-09-30', 1, TRUE, NULL, 20, NULL, 'PAM', 1, 1, '2025-09-30', 0);

-- Branch #40: sub_item/else/else/then => item(..)
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('sub_item_40', 'OtherEntity', 'IntraGroupBank', 'OtherLoan', 1, 0, '2025-09-30', 1, TRUE, NULL, 0, '2027-09-25', 'PAM', 1, 1, '2025-09-30', 0);

-- Branch #41: sub_item/else/else/else/then => item(..)
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('sub_item_41', 'OtherEntity', 'IntraGroupBank', 'OtherLoan', 1, 0, '2025-09-30', 1, TRUE, NULL, 0, '2027-03-29', 'PAM', 1, 1, '2025-09-30', 0);

-- Branch #42: sub_item/else/else/else/else => item(..)
INSERT INTO loan (id, borrower, affiliation, product, balance, rate, next_payment, cycle_months, performing, lvr_at_most_80, risk_weight, encumbered_until, contract_kind, payment, principal_per_period, maturity, minimum_payment) VALUES ('sub_item_42', 'OtherEntity', 'IntraGroupBank', 'OtherLoan', 1, 0, '2025-09-30', 1, TRUE, NULL, 0, NULL, 'PAM', 1, 1, '2025-09-30', 0);

