-- Z3-synthesized branch-covering witness dataset for module `SecuredFunding`
-- Every row below was solved by Z3 to satisfy a distinct leaf branch + domain constraints.

INSERT INTO securedfunding_params (mode, reporting_date) VALUES ('Conservative', '2026-09-30');

-- Branch #1: stock_row/then => "none"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('stock_row_1', 'DebtSecurity', 'CentralBank', 1, 0, 1, FALSE, FALSE, FALSE, TRUE, NULL, 'AssetBacked', 0, NULL, TRUE);

-- Branch #2: stock_row/else/NotesAndCoin => "1.1"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('stock_row_2', 'NotesAndCoin', 'CentralBank', 1, 100, 1, FALSE, FALSE, FALSE, FALSE, NULL, 'AssetBacked', 0, NULL, TRUE);

-- Branch #3: stock_row/else/RbaBalance/then => "1.2.1"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('stock_row_3', 'RbaBalance', 'CentralBank', 1, 100, 1, FALSE, FALSE, FALSE, FALSE, TRUE, 'AssetBacked', 0, NULL, TRUE);

-- Branch #4: stock_row/else/RbaBalance/else => "none"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('stock_row_4', 'RbaBalance', 'CentralBank', 1, 100, 1, FALSE, FALSE, FALSE, FALSE, NULL, 'AssetBacked', 0, NULL, TRUE);

-- Branch #5: stock_row/else/ForeignCentralBankBalance/then => "1.2.2"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('stock_row_5', 'ForeignCentralBankBalance', 'CentralBank', 1, 100, 1, FALSE, FALSE, FALSE, FALSE, TRUE, 'AssetBacked', 0, NULL, TRUE);

-- Branch #6: stock_row/else/ForeignCentralBankBalance/else => "none"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('stock_row_6', 'ForeignCentralBankBalance', 'CentralBank', 1, 100, 1, FALSE, FALSE, FALSE, FALSE, NULL, 'AssetBacked', 0, NULL, TRUE);

-- Branch #7: stock_row/else/DebtSecurity/then/AustralianGovernment => "1.3.1"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('stock_row_7', 'DebtSecurity', 'AustralianGovernment', 1, 0, 1, FALSE, FALSE, FALSE, FALSE, NULL, 'AssetBacked', 0, NULL, TRUE);

-- Branch #8: stock_row/else/DebtSecurity/then/StateCentralBorrowingAuthority => "1.3.2"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('stock_row_8', 'DebtSecurity', 'StateCentralBorrowingAuthority', 1, 0, 1, FALSE, FALSE, FALSE, FALSE, NULL, 'AssetBacked', 0, NULL, TRUE);

-- Branch #9: stock_row/else/DebtSecurity/then/ForeignSovereign => "1.3.3"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('stock_row_9', 'DebtSecurity', 'ForeignSovereign', 1, 0, 1, FALSE, FALSE, FALSE, FALSE, NULL, 'AssetBacked', 0, NULL, TRUE);

-- Branch #10: stock_row/else/DebtSecurity/then/GuaranteedByAustralianGovernment => "1.3.4"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('stock_row_10', 'DebtSecurity', 'GuaranteedByAustralianGovernment', 1, 0, 1, FALSE, FALSE, FALSE, FALSE, NULL, 'AssetBacked', 0, NULL, TRUE);

-- Branch #11: stock_row/else/DebtSecurity/then/GuaranteedByForeignSovereign => "1.3.5"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('stock_row_11', 'DebtSecurity', 'GuaranteedByForeignSovereign', 1, 0, 1, FALSE, FALSE, FALSE, FALSE, NULL, 'AssetBacked', 0, NULL, TRUE);

-- Branch #12: stock_row/else/DebtSecurity/then/CentralBank => "1.3.6"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('stock_row_12', 'DebtSecurity', 'CentralBank', 1, 0, 1, FALSE, FALSE, FALSE, FALSE, NULL, 'AssetBacked', 0, NULL, TRUE);

-- Branch #13: stock_row/else/DebtSecurity/then/PublicSectorEntity => "1.3.7"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('stock_row_13', 'DebtSecurity', 'PublicSectorEntity', 1, 0, 1, FALSE, FALSE, FALSE, FALSE, NULL, 'AssetBacked', 0, NULL, TRUE);

-- Branch #14: stock_row/else/DebtSecurity/then/Supranational => "1.3.8"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('stock_row_14', 'DebtSecurity', 'Supranational', 1, 0, 1, FALSE, FALSE, FALSE, FALSE, NULL, 'AssetBacked', 0, NULL, TRUE);

-- Branch #15: stock_row/else/DebtSecurity/then/otherwise => "none"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('stock_row_15', 'DebtSecurity', 'NonFinancialCorporateIssuer', 1, 0, 1, FALSE, FALSE, FALSE, FALSE, NULL, 'AssetBacked', 0, NULL, TRUE);

-- Branch #16: stock_row/else/DebtSecurity/else/then/then => "1.4.1"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('stock_row_16', 'DebtSecurity', 'CentralBank', 1, 35, 1, TRUE, FALSE, FALSE, FALSE, NULL, 'AssetBacked', 0, NULL, TRUE);

-- Branch #17: stock_row/else/DebtSecurity/else/then/else => "1.4.2"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('stock_row_17', 'DebtSecurity', 'CentralBank', 1, 35, 1, FALSE, FALSE, FALSE, FALSE, NULL, 'AssetBacked', 0, NULL, TRUE);

-- Branch #18: stock_row/else/DebtSecurity/else/else/then => "2.1"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('stock_row_18', 'DebtSecurity', 'CentralBank', 1, 20, 1, FALSE, FALSE, FALSE, FALSE, NULL, 'AssetBacked', 0, NULL, TRUE);

-- Branch #19: stock_row/else/DebtSecurity/else/else/else/then => "2.2"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('stock_row_19', 'DebtSecurity', 'PublicSectorEntity', 1, 20, 1, FALSE, FALSE, FALSE, FALSE, NULL, 'AssetBacked', 0, NULL, TRUE);

-- Branch #20: stock_row/else/DebtSecurity/else/else/else/else/then => "2.3"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('stock_row_20', 'DebtSecurity', 'NonFinancialCorporateIssuer', 1, 20, 1, FALSE, FALSE, FALSE, FALSE, NULL, 'AssetBacked', 0, NULL, TRUE);

-- Branch #21: stock_row/else/DebtSecurity/else/else/else/else/else/then => "3.2"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('stock_row_21', 'DebtSecurity', 'NonFinancialCorporateIssuer', 1, 20, 2, FALSE, FALSE, FALSE, FALSE, NULL, 'AssetBacked', 0, NULL, TRUE);

-- Branch #22: stock_row/else/DebtSecurity/else/else/else/else/else/else => "none"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('stock_row_22', 'DebtSecurity', 'AustralianGovernment', 1, 100, 4, FALSE, FALSE, FALSE, FALSE, NULL, 'AssetBacked', 0, NULL, TRUE);

-- Branch #23: stock_row/else/CoveredBond/then => "2.4"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('stock_row_23', 'CoveredBond', 'CentralBank', 1, 50, 1, FALSE, FALSE, FALSE, FALSE, NULL, 'AssetBacked', 0, NULL, TRUE);

-- Branch #24: stock_row/else/CoveredBond/else => "none"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('stock_row_24', 'CoveredBond', 'CentralBank', 1, 100, 2, FALSE, FALSE, TRUE, FALSE, NULL, 'AssetBacked', 0, NULL, TRUE);

-- Branch #25: stock_row/else/Rmbs/then => "3.1"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('stock_row_25', 'Rmbs', 'CentralBank', 1, 50, 1, FALSE, FALSE, FALSE, FALSE, NULL, 'AssetBacked', 0, NULL, TRUE);

-- Branch #26: stock_row/else/Rmbs/else => "none"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('stock_row_26', 'Rmbs', 'CentralBank', 1, 100, 2, FALSE, FALSE, TRUE, FALSE, NULL, 'AssetBacked', 0, NULL, TRUE);

-- Branch #27: stock_row/else/Equity/then => "3.3"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('stock_row_27', 'Equity', 'NonFinancialCorporateIssuer', 1, 100, 1, FALSE, TRUE, FALSE, FALSE, NULL, 'AssetBacked', 0, NULL, TRUE);

-- Branch #28: stock_row/else/Equity/else => "none"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('stock_row_28', 'Equity', 'CentralBank', 1, 0, 1, FALSE, FALSE, FALSE, FALSE, NULL, 'AssetBacked', 0, NULL, TRUE);

-- Branch #29: clf_row/then => "none"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('clf_row_29', 'DebtSecurity', 'AustralianGovernment', 1, 0, 4, TRUE, FALSE, TRUE, FALSE, TRUE, 'AssetBacked', 0, NULL, TRUE);

-- Branch #30: clf_row/else/SupranationalOrForeignGovernment => "6.1.1"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('clf_row_30', 'RbaBalance', 'NonFinancialCorporateIssuer', 1, 0, 1, TRUE, TRUE, FALSE, FALSE, NULL, 'SupranationalOrForeignGovernment', 0, NULL, TRUE);

-- Branch #31: clf_row/else/Guaranteed => "6.1.2"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('clf_row_31', 'RbaBalance', 'NonFinancialCorporateIssuer', 1, 0, 1, TRUE, TRUE, FALSE, FALSE, NULL, 'Guaranteed', 0, NULL, TRUE);

-- Branch #32: clf_row/else/AdiIssued => "6.1.3"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('clf_row_32', 'RbaBalance', 'NonFinancialCorporateIssuer', 1, 0, 1, TRUE, TRUE, FALSE, FALSE, NULL, 'AdiIssued', 0, NULL, TRUE);

-- Branch #33: clf_row/else/AssetBacked => "6.1.4"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('clf_row_33', 'RbaBalance', 'NonFinancialCorporateIssuer', 1, 0, 1, TRUE, TRUE, FALSE, FALSE, NULL, 'AssetBacked', 0, NULL, TRUE);

-- Branch #34: clf_row/else/OtherPrivate => "6.1.5"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('clf_row_34', 'RbaBalance', 'NonFinancialCorporateIssuer', 1, 0, 1, TRUE, TRUE, FALSE, FALSE, NULL, 'OtherPrivate', 0, NULL, TRUE);

-- Branch #35: clf_row/else/SelfSecuritised => "6.1.6"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('clf_row_35', 'RbaBalance', 'NonFinancialCorporateIssuer', 1, 0, 1, TRUE, TRUE, FALSE, FALSE, NULL, 'SelfSecuritised', 0, NULL, TRUE);

-- Branch #36: clf_row/else/NotClfEligible => "none"
INSERT INTO holding (id, kind, issuer, market_value, risk_weight, rating_grade, domestic_currency, index_constituent, own_or_associated, encumbered, drawable_in_stress, clf_category, value_after_rba_margin, maturity, operational) VALUES ('clf_row_36', 'RbaBalance', 'NonFinancialCorporateIssuer', 1, 0, 1, TRUE, TRUE, FALSE, FALSE, NULL, 'NotClfEligible', 0, NULL, TRUE);

-- Branch #37: unwind/then => <expr>
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('unwind_37', 'Lending', 1, 'ClfEligible', 1, 'DomesticSovereignOrPse', NULL, TRUE, 'REP', '2025-09-30');

-- Branch #38: unwind/else/Funding => <expr>
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('unwind_38', 'Funding', 1, 'HQLA1', 1, 'DomesticSovereignOrPse', TRUE, FALSE, 'REP', '2025-09-30');

-- Branch #39: unwind/else/Lending => <expr>
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('unwind_39', 'Lending', 1, 'HQLA1', 1, 'DomesticSovereignOrPse', TRUE, FALSE, 'REP', '2025-09-30');

-- Branch #40: row/Funding/OffshoreCentralBank => "11.7"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('row_40', 'Funding', 1, 'HQLA1', 1, 'OffshoreCentralBank', NULL, FALSE, 'REP', '2025-09-30');

-- Branch #41: row/Funding/otherwise/HQLA1 => "11.1"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('row_41', 'Funding', 1, 'HQLA1', 1, 'DomesticSovereignOrPse', NULL, FALSE, 'REP', '2025-09-30');

-- Branch #42: row/Funding/otherwise/HQLA2A => "11.2"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('row_42', 'Funding', 1, 'HQLA2A', 1, 'DomesticSovereignOrPse', NULL, FALSE, 'REP', '2025-09-30');

-- Branch #43: row/Funding/otherwise/RmbsHqla2B => "11.3"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('row_43', 'Funding', 1, 'RmbsHqla2B', 1, 'DomesticSovereignOrPse', NULL, FALSE, 'REP', '2025-09-30');

-- Branch #44: row/Funding/otherwise/NonRmbsHqla2B => "11.4"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('row_44', 'Funding', 1, 'NonRmbsHqla2B', 1, 'DomesticSovereignOrPse', NULL, FALSE, 'REP', '2025-09-30');

-- Branch #45: row/Funding/otherwise/ClfEligible/then => "11.5"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('row_45', 'Funding', 1, 'ClfEligible', 1, 'DomesticSovereignOrPse', NULL, FALSE, 'REP', '2025-09-30');

-- Branch #46: row/Funding/otherwise/ClfEligible/else/then => "11.6.1"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('row_46', 'Funding', 1, 'ClfEligible', 1, 'DomesticSovereignOrPse', NULL, FALSE, 'REP', '2025-09-30');

-- Branch #47: row/Funding/otherwise/ClfEligible/else/else => "11.6.2"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('row_47', 'Funding', 1, 'ClfEligible', 1, 'OtherCounterparty', NULL, FALSE, 'REP', '2025-09-30');

-- Branch #48: row/Funding/otherwise/NonHqla/then => "11.6.1"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('row_48', 'Funding', 1, 'NonHqla', 1, 'DomesticSovereignOrPse', NULL, FALSE, 'REP', '2025-09-30');

-- Branch #49: row/Funding/otherwise/NonHqla/else => "11.6.2"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('row_49', 'Funding', 1, 'NonHqla', 1, 'OtherCounterparty', NULL, FALSE, 'REP', '2025-09-30');

-- Branch #50: row/Lending/HQLA1/then => "19.2.1"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('row_50', 'Lending', 1, 'HQLA1', 1, 'DomesticSovereignOrPse', NULL, TRUE, 'REP', '2025-09-30');

-- Branch #51: row/Lending/HQLA1/else => "19.1.1"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('row_51', 'Lending', 1, 'HQLA1', 1, 'DomesticSovereignOrPse', NULL, FALSE, 'REP', '2025-09-30');

-- Branch #52: row/Lending/HQLA2A/then => "19.2.2"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('row_52', 'Lending', 1, 'HQLA2A', 1, 'DomesticSovereignOrPse', NULL, TRUE, 'REP', '2025-09-30');

-- Branch #53: row/Lending/HQLA2A/else => "19.1.2"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('row_53', 'Lending', 1, 'HQLA2A', 1, 'DomesticSovereignOrPse', NULL, FALSE, 'REP', '2025-09-30');

-- Branch #54: row/Lending/RmbsHqla2B/then => "19.2.3"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('row_54', 'Lending', 1, 'RmbsHqla2B', 1, 'DomesticSovereignOrPse', NULL, TRUE, 'REP', '2025-09-30');

-- Branch #55: row/Lending/RmbsHqla2B/else => "19.1.3"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('row_55', 'Lending', 1, 'RmbsHqla2B', 1, 'DomesticSovereignOrPse', NULL, FALSE, 'REP', '2025-09-30');

-- Branch #56: row/Lending/NonRmbsHqla2B/then => "19.2.4"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('row_56', 'Lending', 1, 'NonRmbsHqla2B', 1, 'DomesticSovereignOrPse', NULL, TRUE, 'REP', '2025-09-30');

-- Branch #57: row/Lending/NonRmbsHqla2B/else => "19.1.4"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('row_57', 'Lending', 1, 'NonRmbsHqla2B', 1, 'DomesticSovereignOrPse', NULL, FALSE, 'REP', '2025-09-30');

-- Branch #58: row/Lending/ClfEligible/then => "19.2.6"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('row_58', 'Lending', 1, 'ClfEligible', 1, 'DomesticSovereignOrPse', NULL, TRUE, 'REP', '2025-09-30');

-- Branch #59: row/Lending/ClfEligible/else/then => "19.1.5"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('row_59', 'Lending', 1, 'ClfEligible', 1, 'DomesticSovereignOrPse', NULL, FALSE, 'REP', '2025-09-30');

-- Branch #60: row/Lending/ClfEligible/else/else => "19.1.8"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('row_60', 'Lending', 1, 'ClfEligible', 1, 'DomesticSovereignOrPse', NULL, FALSE, 'REP', '2025-09-30');

-- Branch #61: row/Lending/NonHqla/then => "19.2.6"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('row_61', 'Lending', 1, 'NonHqla', 1, 'DomesticSovereignOrPse', NULL, TRUE, 'REP', '2025-09-30');

-- Branch #62: row/Lending/NonHqla/else => "19.1.8"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('row_62', 'Lending', 1, 'NonHqla', 1, 'DomesticSovereignOrPse', NULL, FALSE, 'REP', '2025-09-30');

-- Branch #63: of_which_row/(otherwise,otherwise,NonHqla) => "none"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('of_which_row_63', 'Lending', 1, 'NonHqla', 1, 'DomesticSovereignOrPse', NULL, FALSE, 'REP', '2025-09-30');

-- Branch #64: of_which_row/(otherwise,otherwise,ClfEligible) => "none"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('of_which_row_64', 'Lending', 1, 'ClfEligible', 1, 'DomesticSovereignOrPse', NULL, FALSE, 'REP', '2025-09-30');

-- Branch #65: of_which_row/(Funding,OffshoreCentralBank,HQLA1) => "11.7.1"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('of_which_row_65', 'Funding', 1, 'HQLA1', 1, 'OffshoreCentralBank', NULL, FALSE, 'REP', '2025-09-30');

-- Branch #66: of_which_row/(Funding,OffshoreCentralBank,HQLA2A) => "11.7.3"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('of_which_row_66', 'Funding', 1, 'HQLA2A', 1, 'OffshoreCentralBank', NULL, FALSE, 'REP', '2025-09-30');

-- Branch #67: of_which_row/(Funding,OffshoreCentralBank,RmbsHqla2B) => "11.7.5"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('of_which_row_67', 'Funding', 1, 'RmbsHqla2B', 1, 'OffshoreCentralBank', NULL, FALSE, 'REP', '2025-09-30');

-- Branch #68: of_which_row/(Funding,OffshoreCentralBank,NonRmbsHqla2B) => "11.7.7"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('of_which_row_68', 'Funding', 1, 'NonRmbsHqla2B', 1, 'OffshoreCentralBank', NULL, FALSE, 'REP', '2025-09-30');

-- Branch #69: of_which_row/otherwise => item(..)
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('of_which_row_69', 'Lending', 1, 'HQLA1', 1, 'DomesticSovereignOrPse', NULL, FALSE, 'REP', '2025-09-30');

-- Branch #70: collateral_row/(otherwise,otherwise,NonHqla) => "none"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('collateral_row_70', 'Lending', 1, 'NonHqla', 1, 'DomesticSovereignOrPse', NULL, FALSE, 'REP', '2025-09-30');

-- Branch #71: collateral_row/(otherwise,otherwise,ClfEligible) => "none"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('collateral_row_71', 'Lending', 1, 'ClfEligible', 1, 'DomesticSovereignOrPse', NULL, FALSE, 'REP', '2025-09-30');

-- Branch #72: collateral_row/(Funding,OffshoreCentralBank,HQLA1) => "11.7.2"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('collateral_row_72', 'Funding', 1, 'HQLA1', 1, 'OffshoreCentralBank', NULL, FALSE, 'REP', '2025-09-30');

-- Branch #73: collateral_row/(Funding,OffshoreCentralBank,HQLA2A) => "11.7.4"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('collateral_row_73', 'Funding', 1, 'HQLA2A', 1, 'OffshoreCentralBank', NULL, FALSE, 'REP', '2025-09-30');

-- Branch #74: collateral_row/(Funding,OffshoreCentralBank,RmbsHqla2B) => "11.7.6"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('collateral_row_74', 'Funding', 1, 'RmbsHqla2B', 1, 'OffshoreCentralBank', NULL, FALSE, 'REP', '2025-09-30');

-- Branch #75: collateral_row/(Funding,OffshoreCentralBank,NonRmbsHqla2B) => "11.7.8"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('collateral_row_75', 'Funding', 1, 'NonRmbsHqla2B', 1, 'OffshoreCentralBank', NULL, FALSE, 'REP', '2025-09-30');

-- Branch #76: collateral_row/otherwise => item(..)
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('collateral_row_76', 'Lending', 1, 'HQLA1', 1, 'DomesticSovereignOrPse', NULL, FALSE, 'REP', '2025-09-30');

-- Branch #77: clf_collateral_row/then => "none"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('clf_collateral_row_77', 'Lending', 1, 'HQLA1', 1, 'OffshoreCentralBank', NULL, FALSE, 'REP', '2025-09-30');

-- Branch #78: clf_collateral_row/else/Funding => "11.5.1"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('clf_collateral_row_78', 'Funding', 1, 'ClfEligible', 1, 'DomesticSovereignOrPse', NULL, FALSE, 'REP', '2025-09-30');

-- Branch #79: clf_collateral_row/else/Lending/then => "none"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('clf_collateral_row_79', 'Lending', 1, 'ClfEligible', 1, 'DomesticSovereignOrPse', NULL, TRUE, 'REP', '2025-09-30');

-- Branch #80: clf_collateral_row/else/Lending/else => "19.1.5.1"
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('clf_collateral_row_80', 'Lending', 1, 'ClfEligible', 1, 'DomesticSovereignOrPse', NULL, FALSE, 'REP', '2025-09-30');

-- Branch #81: contribution/then => <expr>
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('contribution_81', 'Lending', 1, 'HQLA1', 1, 'DomesticSovereignOrPse', NULL, FALSE, 'REP', '2026-10-31');

-- Branch #82: contribution/else => <expr>
INSERT INTO repo (id, leg, cash, collateral_class, collateral_value, counterparty, operational, rehypothecated, contract_kind, maturity) VALUES ('contribution_82', 'Lending', 1, 'HQLA1', 1, 'DomesticSovereignOrPse', NULL, FALSE, 'REP', '2025-09-30');

