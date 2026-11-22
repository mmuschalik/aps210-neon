-- Z3-synthesized branch-covering witness dataset for module `WholesaleDeposits`
-- Every row below was solved by Z3 to satisfy a distinct leaf branch + domain constraints.

INSERT INTO wholesaledeposits_params (mode, reporting_date) VALUES ('Conservative', '2026-09-30');

-- Branch #1: operational_amount/then => min(..)
INSERT INTO customer (id, kind) VALUES ('operational_amount_1_r__holder', 'SovereignCentralBankPseMdb');
INSERT INTO deposit (id, holder, balance, within_30_days, called, fcs_eligible, operational_service, operational_balance) VALUES ('operational_amount_1', 'operational_amount_1_r__holder', 1, FALSE, FALSE, NULL, TRUE, NULL);

-- Branch #2: operational_amount/else => <expr>
INSERT INTO customer (id, kind) VALUES ('operational_amount_2_r__holder', 'SovereignCentralBankPseMdb');
INSERT INTO deposit (id, holder, balance, within_30_days, called, fcs_eligible, operational_service, operational_balance) VALUES ('operational_amount_2', 'operational_amount_2_r__holder', 1, FALSE, FALSE, NULL, NULL, NULL);

-- Branch #3: prefix/NonFinancialCorporate => "9.2"
INSERT INTO customer (id, kind) VALUES ('prefix_3', 'NonFinancialCorporate');

-- Branch #4: prefix/SovereignCentralBankPseMdb => "9.3"
INSERT INTO customer (id, kind) VALUES ('prefix_4', 'SovereignCentralBankPseMdb');

-- Branch #5: prefix/Bank => "9.4"
INSERT INTO customer (id, kind) VALUES ('prefix_5', 'Bank');

-- Branch #6: prefix/OtherFinancialInstitution => "9.5"
INSERT INTO customer (id, kind) VALUES ('prefix_6', 'OtherFinancialInstitution');

-- Branch #7: prefix/OtherLegalEntity => "9.5"
INSERT INTO customer (id, kind) VALUES ('prefix_7', 'OtherLegalEntity');

