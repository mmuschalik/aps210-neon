-- Z3-synthesized branch-covering witness dataset for module `Commitments`
-- Every row below was solved by Z3 to satisfy a distinct leaf branch + domain constraints.

INSERT INTO commitments_params (mode, reporting_date) VALUES ('Conservative', '2026-09-30');

-- Branch #1: facility_row/then => "none"
INSERT INTO facility (id, counterparty, kind, committed, undrawn, drawable_within_30_days, mortgage_redraw, funds_vehicle) VALUES ('facility_row_1', 'SovereignCentralBankPseMdb', 'CreditFacility', FALSE, 3782145, FALSE, FALSE, FALSE);

-- Branch #2: facility_row/else/then => "16.1"
INSERT INTO facility (id, counterparty, kind, committed, undrawn, drawable_within_30_days, mortgage_redraw, funds_vehicle) VALUES ('facility_row_2', 'SovereignCentralBankPseMdb', 'CreditFacility', FALSE, 3782145, TRUE, FALSE, FALSE);

-- Branch #3: facility_row/else/else/then => "14.6.2"
INSERT INTO facility (id, counterparty, kind, committed, undrawn, drawable_within_30_days, mortgage_redraw, funds_vehicle) VALUES ('facility_row_3', 'SovereignCentralBankPseMdb', 'CreditFacility', TRUE, 3782209, TRUE, FALSE, TRUE);

-- Branch #4: facility_row/else/else/else/RetailCustomer => "14.1.1"
INSERT INTO facility (id, counterparty, kind, committed, undrawn, drawable_within_30_days, mortgage_redraw, funds_vehicle) VALUES ('facility_row_4', 'RetailCustomer', 'CreditFacility', TRUE, 1, TRUE, FALSE, FALSE);

-- Branch #5: facility_row/else/else/else/Sme => "14.2.1"
INSERT INTO facility (id, counterparty, kind, committed, undrawn, drawable_within_30_days, mortgage_redraw, funds_vehicle) VALUES ('facility_row_5', 'Sme', 'CreditFacility', TRUE, 1, TRUE, FALSE, FALSE);

-- Branch #6: facility_row/else/else/else/NonFinancialCorporate/then => "14.3.1"
INSERT INTO facility (id, counterparty, kind, committed, undrawn, drawable_within_30_days, mortgage_redraw, funds_vehicle) VALUES ('facility_row_6', 'NonFinancialCorporate', 'CreditFacility', TRUE, 1, TRUE, FALSE, FALSE);

-- Branch #7: facility_row/else/else/else/NonFinancialCorporate/else => "14.3.2"
INSERT INTO facility (id, counterparty, kind, committed, undrawn, drawable_within_30_days, mortgage_redraw, funds_vehicle) VALUES ('facility_row_7', 'NonFinancialCorporate', 'LiquidityFacility', TRUE, 1, TRUE, FALSE, FALSE);

-- Branch #8: facility_row/else/else/else/SovereignCentralBankPseMdb/then => "14.4.1"
INSERT INTO facility (id, counterparty, kind, committed, undrawn, drawable_within_30_days, mortgage_redraw, funds_vehicle) VALUES ('facility_row_8', 'SovereignCentralBankPseMdb', 'CreditFacility', TRUE, 1, TRUE, FALSE, FALSE);

-- Branch #9: facility_row/else/else/else/SovereignCentralBankPseMdb/else => "14.4.2"
INSERT INTO facility (id, counterparty, kind, committed, undrawn, drawable_within_30_days, mortgage_redraw, funds_vehicle) VALUES ('facility_row_9', 'SovereignCentralBankPseMdb', 'LiquidityFacility', TRUE, 1, TRUE, FALSE, FALSE);

-- Branch #10: facility_row/else/else/else/RegulatedBank => "14.5.1"
INSERT INTO facility (id, counterparty, kind, committed, undrawn, drawable_within_30_days, mortgage_redraw, funds_vehicle) VALUES ('facility_row_10', 'RegulatedBank', 'CreditFacility', TRUE, 1, TRUE, FALSE, FALSE);

-- Branch #11: facility_row/else/else/else/OtherFinancialInstitution/then => "14.6.1"
INSERT INTO facility (id, counterparty, kind, committed, undrawn, drawable_within_30_days, mortgage_redraw, funds_vehicle) VALUES ('facility_row_11', 'OtherFinancialInstitution', 'CreditFacility', TRUE, 1, TRUE, FALSE, FALSE);

-- Branch #12: facility_row/else/else/else/OtherFinancialInstitution/else => "14.6.2"
INSERT INTO facility (id, counterparty, kind, committed, undrawn, drawable_within_30_days, mortgage_redraw, funds_vehicle) VALUES ('facility_row_12', 'OtherFinancialInstitution', 'LiquidityFacility', TRUE, 1, TRUE, FALSE, FALSE);

-- Branch #13: facility_row/else/else/else/OtherLegalEntity => "14.7.1"
INSERT INTO facility (id, counterparty, kind, committed, undrawn, drawable_within_30_days, mortgage_redraw, funds_vehicle) VALUES ('facility_row_13', 'OtherLegalEntity', 'CreditFacility', TRUE, 1, TRUE, FALSE, FALSE);

-- Branch #14: facility/then => <expr>
INSERT INTO facility (id, counterparty, kind, committed, undrawn, drawable_within_30_days, mortgage_redraw, funds_vehicle) VALUES ('facility_14', 'SovereignCentralBankPseMdb', 'CreditFacility', FALSE, 1, NULL, FALSE, FALSE);

-- Branch #15: facility/else => <expr>
INSERT INTO facility (id, counterparty, kind, committed, undrawn, drawable_within_30_days, mortgage_redraw, funds_vehicle) VALUES ('facility_15', 'RetailCustomer', 'CreditFacility', TRUE, 1, TRUE, FALSE, TRUE);

-- Branch #16: commitment_row/RegulatedBank => "15.1"
INSERT INTO lendingcommitment (id, counterparty, amount) VALUES ('commitment_row_16', 'RegulatedBank', 1);

-- Branch #17: commitment_row/OtherFinancialInstitution => "15.1"
INSERT INTO lendingcommitment (id, counterparty, amount) VALUES ('commitment_row_17', 'OtherFinancialInstitution', 1);

-- Branch #18: commitment_row/RetailCustomer => "15.2"
INSERT INTO lendingcommitment (id, counterparty, amount) VALUES ('commitment_row_18', 'RetailCustomer', 1);

-- Branch #19: commitment_row/Sme => "15.3"
INSERT INTO lendingcommitment (id, counterparty, amount) VALUES ('commitment_row_19', 'Sme', 1);

-- Branch #20: commitment_row/NonFinancialCorporate => "15.4"
INSERT INTO lendingcommitment (id, counterparty, amount) VALUES ('commitment_row_20', 'NonFinancialCorporate', 1);

-- Branch #21: commitment_row/SovereignCentralBankPseMdb => "15.5"
INSERT INTO lendingcommitment (id, counterparty, amount) VALUES ('commitment_row_21', 'SovereignCentralBankPseMdb', 1);

-- Branch #22: commitment_row/OtherLegalEntity => "15.5"
INSERT INTO lendingcommitment (id, counterparty, amount) VALUES ('commitment_row_22', 'OtherLegalEntity', 1);

-- Branch #23: commitment/then => <expr>
INSERT INTO lendingcommitment (id, counterparty, amount) VALUES ('commitment_23', 'SovereignCentralBankPseMdb', 1);

-- Branch #24: commitment/else => <expr>
INSERT INTO lendingcommitment (id, counterparty, amount) VALUES ('commitment_24', 'OtherFinancialInstitution', 1);

-- Branch #25: obligation_row/TradeFinance => "16.2"
INSERT INTO contingentobligation (id, kind, amount) VALUES ('obligation_row_25', 'TradeFinance', 1);

-- Branch #26: obligation_row/Guarantee => "16.3"
INSERT INTO contingentobligation (id, kind, amount) VALUES ('obligation_row_26', 'Guarantee', 1);

-- Branch #27: obligation_row/ShortTermDebtBuyback => "16.4"
INSERT INTO contingentobligation (id, kind, amount) VALUES ('obligation_row_27', 'ShortTermDebtBuyback', 1);

-- Branch #28: obligation_row/LongTermDebtBuyback => "16.5"
INSERT INTO contingentobligation (id, kind, amount) VALUES ('obligation_row_28', 'LongTermDebtBuyback', 1);

-- Branch #29: obligation_row/AffiliatedDealerBuyback => "16.6"
INSERT INTO contingentobligation (id, kind, amount) VALUES ('obligation_row_29', 'AffiliatedDealerBuyback', 1);

-- Branch #30: obligation_row/StructuredProductsAndFunds => "16.7"
INSERT INTO contingentobligation (id, kind, amount) VALUES ('obligation_row_30', 'StructuredProductsAndFunds', 1);

-- Branch #31: obligation_row/OtherNonContractual => "16.8"
INSERT INTO contingentobligation (id, kind, amount) VALUES ('obligation_row_31', 'OtherNonContractual', 1);

-- Branch #32: obligation_row/OtherContractual => "16.9"
INSERT INTO contingentobligation (id, kind, amount) VALUES ('obligation_row_32', 'OtherContractual', 1);

-- Branch #33: obligation/then => <expr>
INSERT INTO contingentobligation (id, kind, amount) VALUES ('obligation_33', 'LongTermDebtBuyback', 1);

-- Branch #34: obligation/else => <expr>
INSERT INTO contingentobligation (id, kind, amount) VALUES ('obligation_34', 'TradeFinance', 1);

