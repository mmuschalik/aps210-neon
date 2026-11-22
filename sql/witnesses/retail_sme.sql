-- Z3-synthesized branch-covering witness dataset for module `RetailAndSmeDeposits`
-- Every row below was solved by Z3 to satisfy a distinct leaf branch + domain constraints.

INSERT INTO retailandsmedeposits_params (mode, reporting_date) VALUES ('Conservative', '2026-09-30');

-- Branch #1: class/NaturalPerson => Retail
INSERT INTO customer (id, kind, established_relationship, sme_group, managed_as_retail, pie_qualifies) VALUES ('class_1', 'NaturalPerson', NULL, NULL, NULL, NULL);

-- Branch #2: class/Smsf => RetailLessStable
INSERT INTO customer (id, kind, established_relationship, sme_group, managed_as_retail, pie_qualifies) VALUES ('class_2', 'Smsf', NULL, NULL, NULL, NULL);

-- Branch #3: class/PersonalInvestmentEntity/then => RetailLessStable
INSERT INTO customer (id, kind, established_relationship, sme_group, managed_as_retail, pie_qualifies) VALUES ('class_3', 'PersonalInvestmentEntity', NULL, NULL, NULL, TRUE);

-- Branch #4: class/PersonalInvestmentEntity/else => FinancialInstitution
INSERT INTO customer (id, kind, established_relationship, sme_group, managed_as_retail, pie_qualifies) VALUES ('class_4', 'PersonalInvestmentEntity', NULL, NULL, NULL, NULL);

-- Branch #5: class/Sme => <expr>
INSERT INTO customer (id, kind, established_relationship, sme_group, managed_as_retail, pie_qualifies) VALUES ('class_5', 'Sme', NULL, NULL, NULL, NULL);

-- Branch #6: class/NonFinancialCorporate => NonFinancialCorporate
INSERT INTO customer (id, kind, established_relationship, sme_group, managed_as_retail, pie_qualifies) VALUES ('class_6', 'NonFinancialCorporate', NULL, NULL, NULL, NULL);

-- Branch #7: class/FinancialInstitution => FinancialInstitution
INSERT INTO customer (id, kind, established_relationship, sme_group, managed_as_retail, pie_qualifies) VALUES ('class_7', 'FinancialInstitution', NULL, NULL, NULL, NULL);

-- Branch #8: horizon/UMP => Demand
INSERT INTO customer (id, kind, established_relationship, sme_group, managed_as_retail, pie_qualifies) VALUES ('horizon_8_r__holder', 'NonFinancialCorporate', NULL, NULL, NULL, NULL);
INSERT INTO deposit (id, holder, product_class, balance, fcs_eligible, transactional, online, rate_driven, contract_kind, notice_days, grace_end, maturity, early_withdrawal) VALUES ('horizon_8', 'horizon_8_r__holder', '', 1, NULL, NULL, NULL, NULL, 'UMP', 0, NULL, '2025-09-30', NULL);

-- Branch #9: horizon/CLM/then => Demand
INSERT INTO customer (id, kind, established_relationship, sme_group, managed_as_retail, pie_qualifies) VALUES ('horizon_9_r__holder', 'NonFinancialCorporate', NULL, NULL, NULL, NULL);
INSERT INTO deposit (id, holder, product_class, balance, fcs_eligible, transactional, online, rate_driven, contract_kind, notice_days, grace_end, maturity, early_withdrawal) VALUES ('horizon_9', 'horizon_9_r__holder', '', 1, NULL, NULL, NULL, NULL, 'CLM', 0, '2026-09-30', '2025-09-30', NULL);

-- Branch #10: horizon/CLM/else/some/then => Called
INSERT INTO customer (id, kind, established_relationship, sme_group, managed_as_retail, pie_qualifies) VALUES ('horizon_10_r__holder', 'NonFinancialCorporate', NULL, NULL, NULL, NULL);
INSERT INTO deposit (id, holder, product_class, balance, fcs_eligible, transactional, online, rate_driven, contract_kind, notice_days, grace_end, maturity, early_withdrawal) VALUES ('horizon_10', 'horizon_10_r__holder', '', 1, NULL, NULL, NULL, NULL, 'CLM', 0, NULL, '2025-09-30', NULL);

-- Branch #11: horizon/CLM/else/some/else => Excluded
INSERT INTO customer (id, kind, established_relationship, sme_group, managed_as_retail, pie_qualifies) VALUES ('horizon_11_r__holder', 'NonFinancialCorporate', NULL, NULL, NULL, NULL);
INSERT INTO deposit (id, holder, product_class, balance, fcs_eligible, transactional, online, rate_driven, contract_kind, notice_days, grace_end, maturity, early_withdrawal) VALUES ('horizon_11', 'horizon_11_r__holder', '', 1, NULL, NULL, NULL, NULL, 'CLM', 0, NULL, '2025-09-30', NULL);

-- Branch #12: horizon/CLM/else/none/then => Excluded
INSERT INTO customer (id, kind, established_relationship, sme_group, managed_as_retail, pie_qualifies) VALUES ('horizon_12_r__holder', 'NonFinancialCorporate', NULL, NULL, NULL, NULL);
INSERT INTO deposit (id, holder, product_class, balance, fcs_eligible, transactional, online, rate_driven, contract_kind, notice_days, grace_end, maturity, early_withdrawal) VALUES ('horizon_12', 'horizon_12_r__holder', '', 1, NULL, NULL, NULL, NULL, 'CLM', 31, NULL, '2025-09-30', NULL);

-- Branch #13: horizon/CLM/else/none/else => Demand
INSERT INTO customer (id, kind, established_relationship, sme_group, managed_as_retail, pie_qualifies) VALUES ('horizon_13_r__holder', 'NonFinancialCorporate', NULL, NULL, NULL, NULL);
INSERT INTO deposit (id, holder, product_class, balance, fcs_eligible, transactional, online, rate_driven, contract_kind, notice_days, grace_end, maturity, early_withdrawal) VALUES ('horizon_13', 'horizon_13_r__holder', '', 1, NULL, NULL, NULL, NULL, 'CLM', 0, NULL, '2025-09-30', NULL);

-- Branch #14: horizon/PAM/then => Demand
INSERT INTO customer (id, kind, established_relationship, sme_group, managed_as_retail, pie_qualifies) VALUES ('horizon_14_r__holder', 'NonFinancialCorporate', NULL, NULL, NULL, NULL);
INSERT INTO deposit (id, holder, product_class, balance, fcs_eligible, transactional, online, rate_driven, contract_kind, notice_days, grace_end, maturity, early_withdrawal) VALUES ('horizon_14', 'horizon_14_r__holder', '', 1, NULL, NULL, NULL, NULL, 'PAM', 0, NULL, '2025-09-30', NULL);

-- Branch #15: horizon/PAM/else/then => Demand
INSERT INTO customer (id, kind, established_relationship, sme_group, managed_as_retail, pie_qualifies) VALUES ('horizon_15_r__holder', 'NonFinancialCorporate', NULL, NULL, NULL, NULL);
INSERT INTO deposit (id, holder, product_class, balance, fcs_eligible, transactional, online, rate_driven, contract_kind, notice_days, grace_end, maturity, early_withdrawal) VALUES ('horizon_15', 'horizon_15_r__holder', '', 1, NULL, NULL, NULL, NULL, 'PAM', 0, NULL, '2026-10-31', 'Permitted');

-- Branch #16: horizon/PAM/else/else => Excluded
INSERT INTO customer (id, kind, established_relationship, sme_group, managed_as_retail, pie_qualifies) VALUES ('horizon_16_r__holder', 'NonFinancialCorporate', NULL, NULL, NULL, NULL);
INSERT INTO deposit (id, holder, product_class, balance, fcs_eligible, transactional, online, rate_driven, contract_kind, notice_days, grace_end, maturity, early_withdrawal) VALUES ('horizon_16', 'horizon_16_r__holder', '', 1, NULL, NULL, NULL, NULL, 'PAM', 0, NULL, '2026-10-31', 'SignificantPenalty');

-- Branch #17: earliest_redemption/UMP => reporting_date
INSERT INTO customer (id, kind, established_relationship, sme_group, managed_as_retail, pie_qualifies) VALUES ('earliest_redemption_17_r__holder', 'NonFinancialCorporate', NULL, NULL, NULL, NULL);
INSERT INTO deposit (id, holder, product_class, balance, fcs_eligible, transactional, online, rate_driven, contract_kind, notice_days, grace_end, maturity, early_withdrawal) VALUES ('earliest_redemption_17', 'earliest_redemption_17_r__holder', '', 1, NULL, NULL, NULL, NULL, 'UMP', 0, NULL, '2025-09-30', NULL);

-- Branch #18: earliest_redemption/CLM/then => reporting_date
INSERT INTO customer (id, kind, established_relationship, sme_group, managed_as_retail, pie_qualifies) VALUES ('earliest_redemption_18_r__holder', 'NonFinancialCorporate', NULL, NULL, NULL, NULL);
INSERT INTO deposit (id, holder, product_class, balance, fcs_eligible, transactional, online, rate_driven, contract_kind, notice_days, grace_end, maturity, early_withdrawal) VALUES ('earliest_redemption_18', 'earliest_redemption_18_r__holder', '', 1, NULL, NULL, NULL, NULL, 'CLM', 0, '2026-09-30', '2025-09-30', NULL);

-- Branch #19: earliest_redemption/CLM/else/some => r
INSERT INTO customer (id, kind, established_relationship, sme_group, managed_as_retail, pie_qualifies) VALUES ('earliest_redemption_19_r__holder', 'NonFinancialCorporate', NULL, NULL, NULL, NULL);
INSERT INTO deposit (id, holder, product_class, balance, fcs_eligible, transactional, online, rate_driven, contract_kind, notice_days, grace_end, maturity, early_withdrawal) VALUES ('earliest_redemption_19', 'earliest_redemption_19_r__holder', '', 1, NULL, NULL, NULL, NULL, 'CLM', 0, NULL, '2025-09-30', NULL);

-- Branch #20: earliest_redemption/CLM/else/none => <expr>
INSERT INTO customer (id, kind, established_relationship, sme_group, managed_as_retail, pie_qualifies) VALUES ('earliest_redemption_20_r__holder', 'NonFinancialCorporate', NULL, NULL, NULL, NULL);
INSERT INTO deposit (id, holder, product_class, balance, fcs_eligible, transactional, online, rate_driven, contract_kind, notice_days, grace_end, maturity, early_withdrawal) VALUES ('earliest_redemption_20', 'earliest_redemption_20_r__holder', '', 1, NULL, NULL, NULL, NULL, 'CLM', 0, NULL, '2025-09-30', NULL);

-- Branch #21: earliest_redemption/PAM/then => reporting_date
INSERT INTO customer (id, kind, established_relationship, sme_group, managed_as_retail, pie_qualifies) VALUES ('earliest_redemption_21_r__holder', 'NonFinancialCorporate', NULL, NULL, NULL, NULL);
INSERT INTO deposit (id, holder, product_class, balance, fcs_eligible, transactional, online, rate_driven, contract_kind, notice_days, grace_end, maturity, early_withdrawal) VALUES ('earliest_redemption_21', 'earliest_redemption_21_r__holder', '', 1, NULL, NULL, NULL, NULL, 'PAM', 0, NULL, '2025-09-30', 'SignificantPenalty');

-- Branch #22: earliest_redemption/PAM/else => maturity
INSERT INTO customer (id, kind, established_relationship, sme_group, managed_as_retail, pie_qualifies) VALUES ('earliest_redemption_22_r__holder', 'NonFinancialCorporate', NULL, NULL, NULL, NULL);
INSERT INTO deposit (id, holder, product_class, balance, fcs_eligible, transactional, online, rate_driven, contract_kind, notice_days, grace_end, maturity, early_withdrawal) VALUES ('earliest_redemption_22', 'earliest_redemption_22_r__holder', '', 1, NULL, NULL, NULL, NULL, 'PAM', 0, NULL, '2026-09-30', 'SignificantPenalty');

-- Branch #23: lcr_rate/Excluded => 0%
INSERT INTO customer (id, kind, established_relationship, sme_group, managed_as_retail, pie_qualifies) VALUES ('lcr_rate_23_r__holder', 'NonFinancialCorporate', NULL, NULL, NULL, NULL);
INSERT INTO deposit (id, holder, product_class, balance, fcs_eligible, transactional, online, rate_driven, contract_kind, notice_days, grace_end, maturity, early_withdrawal) VALUES ('lcr_rate_23', 'lcr_rate_23_r__holder', '', 1, NULL, NULL, NULL, NULL, 'PAM', 0, NULL, '2026-10-31', 'SignificantPenalty');

-- Branch #24: lcr_rate/Called => 100%
INSERT INTO customer (id, kind, established_relationship, sme_group, managed_as_retail, pie_qualifies) VALUES ('lcr_rate_24_r__holder', 'NonFinancialCorporate', NULL, NULL, NULL, NULL);
INSERT INTO deposit (id, holder, product_class, balance, fcs_eligible, transactional, online, rate_driven, contract_kind, notice_days, grace_end, maturity, early_withdrawal) VALUES ('lcr_rate_24', 'lcr_rate_24_r__holder', '', 1, NULL, NULL, NULL, NULL, 'CLM', 0, NULL, '2025-09-30', NULL);

-- Branch #25: lcr_rate/Demand => <expr>
INSERT INTO customer (id, kind, established_relationship, sme_group, managed_as_retail, pie_qualifies) VALUES ('lcr_rate_25_r__holder', 'NonFinancialCorporate', NULL, NULL, NULL, NULL);
INSERT INTO deposit (id, holder, product_class, balance, fcs_eligible, transactional, online, rate_driven, contract_kind, notice_days, grace_end, maturity, early_withdrawal) VALUES ('lcr_rate_25', 'lcr_rate_25_r__holder', '', 1, NULL, NULL, NULL, NULL, 'UMP', 0, NULL, '2025-09-30', 'SignificantPenalty');

