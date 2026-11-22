-- Z3-synthesized branch-covering witness dataset for module `Derivatives`
-- Every row below was solved by Z3 to satisfy a distinct leaf branch + domain constraints.

INSERT INTO derivatives_params (mode, reporting_date) VALUES ('Conservative', '2026-09-30');

-- Branch #1: net_in/then => max(..)
INSERT INTO nettingset (id, counterparty, netting_agreement, collateral_received, required_from_counterparty, received_segregated, hqla_substitutable, due_not_called, downgrade_3_notches, posted_non_hqla1, received_reusable, variation_margin_posted, cash_vm_received_eligible, initial_margin_posted) VALUES ('net_in_1', '', TRUE, 0, 0, FALSE, 0, 0, 0, 0, 0, 0, 0, 0);

-- Branch #2: net_in/else => <expr>
INSERT INTO nettingset (id, counterparty, netting_agreement, collateral_received, required_from_counterparty, received_segregated, hqla_substitutable, due_not_called, downgrade_3_notches, posted_non_hqla1, received_reusable, variation_margin_posted, cash_vm_received_eligible, initial_margin_posted) VALUES ('net_in_2', '', NULL, 0, 0, FALSE, 0, 0, 0, 0, 0, 0, 0, 0);

-- Branch #3: net_out/then => max(..)
INSERT INTO nettingset (id, counterparty, netting_agreement, collateral_received, required_from_counterparty, received_segregated, hqla_substitutable, due_not_called, downgrade_3_notches, posted_non_hqla1, received_reusable, variation_margin_posted, cash_vm_received_eligible, initial_margin_posted) VALUES ('net_out_3', '', TRUE, 0, 0, FALSE, 0, 0, 0, 0, 0, 0, 0, 0);

-- Branch #4: net_out/else => <expr>
INSERT INTO nettingset (id, counterparty, netting_agreement, collateral_received, required_from_counterparty, received_segregated, hqla_substitutable, due_not_called, downgrade_3_notches, posted_non_hqla1, received_reusable, variation_margin_posted, cash_vm_received_eligible, initial_margin_posted) VALUES ('net_out_4', '', NULL, 0, 0, FALSE, 0, 0, 0, 0, 0, 0, 0, 0);

-- Branch #5: excess_callable/then => <expr>
INSERT INTO nettingset (id, counterparty, netting_agreement, collateral_received, required_from_counterparty, received_segregated, hqla_substitutable, due_not_called, downgrade_3_notches, posted_non_hqla1, received_reusable, variation_margin_posted, cash_vm_received_eligible, initial_margin_posted) VALUES ('excess_callable_5', '', NULL, 0, 0, TRUE, 0, 0, 0, 0, 0, 0, 0, 0);

-- Branch #6: excess_callable/else => max(..)
INSERT INTO nettingset (id, counterparty, netting_agreement, collateral_received, required_from_counterparty, received_segregated, hqla_substitutable, due_not_called, downgrade_3_notches, posted_non_hqla1, received_reusable, variation_margin_posted, cash_vm_received_eligible, initial_margin_posted) VALUES ('excess_callable_6', '', NULL, 0, 0, FALSE, 0, 0, 0, 0, 0, 0, 0, 0);

-- Branch #7: substitutable/then => <expr>
INSERT INTO nettingset (id, counterparty, netting_agreement, collateral_received, required_from_counterparty, received_segregated, hqla_substitutable, due_not_called, downgrade_3_notches, posted_non_hqla1, received_reusable, variation_margin_posted, cash_vm_received_eligible, initial_margin_posted) VALUES ('substitutable_7', '', NULL, 0, 0, TRUE, 0, 0, 0, 0, 0, 0, 0, 0);

-- Branch #8: substitutable/else => <expr>
INSERT INTO nettingset (id, counterparty, netting_agreement, collateral_received, required_from_counterparty, received_segregated, hqla_substitutable, due_not_called, downgrade_3_notches, posted_non_hqla1, received_reusable, variation_margin_posted, cash_vm_received_eligible, initial_margin_posted) VALUES ('substitutable_8', '', NULL, 0, 0, FALSE, 0, 0, 0, 0, 0, 0, 0, 0);

-- Branch #9: liabilities/then => max(..)
INSERT INTO nettingset (id, counterparty, netting_agreement, collateral_received, required_from_counterparty, received_segregated, hqla_substitutable, due_not_called, downgrade_3_notches, posted_non_hqla1, received_reusable, variation_margin_posted, cash_vm_received_eligible, initial_margin_posted) VALUES ('liabilities_9', '', TRUE, 0, 0, FALSE, 0, 0, 0, 0, 0, 0, 0, 0);

-- Branch #10: liabilities/else => sum(..)
INSERT INTO nettingset (id, counterparty, netting_agreement, collateral_received, required_from_counterparty, received_segregated, hqla_substitutable, due_not_called, downgrade_3_notches, posted_non_hqla1, received_reusable, variation_margin_posted, cash_vm_received_eligible, initial_margin_posted) VALUES ('liabilities_10', '', NULL, 0, 0, FALSE, 0, 0, 0, 0, 0, 0, 0, 0);

-- Branch #11: assets/then => max(..)
INSERT INTO nettingset (id, counterparty, netting_agreement, collateral_received, required_from_counterparty, received_segregated, hqla_substitutable, due_not_called, downgrade_3_notches, posted_non_hqla1, received_reusable, variation_margin_posted, cash_vm_received_eligible, initial_margin_posted) VALUES ('assets_11', '', TRUE, 0, 0, FALSE, 0, 0, 0, 0, 0, 0, 0, 0);

-- Branch #12: assets/else => sum(..)
INSERT INTO nettingset (id, counterparty, netting_agreement, collateral_received, required_from_counterparty, received_segregated, hqla_substitutable, due_not_called, downgrade_3_notches, posted_non_hqla1, received_reusable, variation_margin_posted, cash_vm_received_eligible, initial_margin_posted) VALUES ('assets_12', '', NULL, 0, 0, FALSE, 0, 0, 0, 0, 0, 0, 0, 0);

-- Branch #13: cash_flows/then => net_out(..)
INSERT INTO nettingset (id, counterparty, netting_agreement, collateral_received, required_from_counterparty, received_segregated, hqla_substitutable, due_not_called, downgrade_3_notches, posted_non_hqla1, received_reusable, variation_margin_posted, cash_vm_received_eligible, initial_margin_posted) VALUES ('cash_flows_13', '', NULL, 0, 0, FALSE, 0, 0, 0, 0, 0, 0, 0, 0);

-- Branch #14: cash_flows/else/then => net_in(..)
INSERT INTO nettingset (id, counterparty, netting_agreement, collateral_received, required_from_counterparty, received_segregated, hqla_substitutable, due_not_called, downgrade_3_notches, posted_non_hqla1, received_reusable, variation_margin_posted, cash_vm_received_eligible, initial_margin_posted) VALUES ('cash_flows_14', '', NULL, 0, 0, FALSE, 0, 0, 0, 0, 0, 0, 0, 0);

-- Branch #15: cash_flows/else/else => <expr>
INSERT INTO nettingset (id, counterparty, netting_agreement, collateral_received, required_from_counterparty, received_segregated, hqla_substitutable, due_not_called, downgrade_3_notches, posted_non_hqla1, received_reusable, variation_margin_posted, cash_vm_received_eligible, initial_margin_posted) VALUES ('cash_flows_15', '', NULL, 0, 0, FALSE, 0, 0, 0, 0, 0, 0, 0, 0);

-- Branch #16: collateral/then => <expr>
INSERT INTO nettingset (id, counterparty, netting_agreement, collateral_received, required_from_counterparty, received_segregated, hqla_substitutable, due_not_called, downgrade_3_notches, posted_non_hqla1, received_reusable, variation_margin_posted, cash_vm_received_eligible, initial_margin_posted) VALUES ('collateral_16', '', NULL, 0, 0, FALSE, 0, 0, 0, 0, 0, 0, 0, 0);

-- Branch #17: collateral/else/then => excess_callable(..)
INSERT INTO nettingset (id, counterparty, netting_agreement, collateral_received, required_from_counterparty, received_segregated, hqla_substitutable, due_not_called, downgrade_3_notches, posted_non_hqla1, received_reusable, variation_margin_posted, cash_vm_received_eligible, initial_margin_posted) VALUES ('collateral_17', '', NULL, 0, 0, FALSE, 0, 0, 0, 0, 0, 0, 0, 0);

-- Branch #18: collateral/else/else/then => <expr>
INSERT INTO nettingset (id, counterparty, netting_agreement, collateral_received, required_from_counterparty, received_segregated, hqla_substitutable, due_not_called, downgrade_3_notches, posted_non_hqla1, received_reusable, variation_margin_posted, cash_vm_received_eligible, initial_margin_posted) VALUES ('collateral_18', '', NULL, 0, 0, FALSE, 0, 0, 0, 0, 0, 0, 0, 0);

-- Branch #19: collateral/else/else/else/then => substitutable(..)
INSERT INTO nettingset (id, counterparty, netting_agreement, collateral_received, required_from_counterparty, received_segregated, hqla_substitutable, due_not_called, downgrade_3_notches, posted_non_hqla1, received_reusable, variation_margin_posted, cash_vm_received_eligible, initial_margin_posted) VALUES ('collateral_19', '', NULL, 0, 0, FALSE, 0, 0, 0, 0, 0, 0, 0, 0);

-- Branch #20: collateral/else/else/else/else/then => posted_valuation_base(..)
INSERT INTO nettingset (id, counterparty, netting_agreement, collateral_received, required_from_counterparty, received_segregated, hqla_substitutable, due_not_called, downgrade_3_notches, posted_non_hqla1, received_reusable, variation_margin_posted, cash_vm_received_eligible, initial_margin_posted) VALUES ('collateral_20', '', NULL, 0, 0, FALSE, 0, 0, 0, 0, 0, 0, 0, 0);

-- Branch #21: collateral/else/else/else/else/else => <expr>
INSERT INTO nettingset (id, counterparty, netting_agreement, collateral_received, required_from_counterparty, received_segregated, hqla_substitutable, due_not_called, downgrade_3_notches, posted_non_hqla1, received_reusable, variation_margin_posted, cash_vm_received_eligible, initial_margin_posted) VALUES ('collateral_21', '', NULL, 0, 0, FALSE, 0, 0, 0, 0, 0, 0, 0, 0);

-- Branch #22: nsfr/then => liabilities(..)
INSERT INTO nettingset (id, counterparty, netting_agreement, collateral_received, required_from_counterparty, received_segregated, hqla_substitutable, due_not_called, downgrade_3_notches, posted_non_hqla1, received_reusable, variation_margin_posted, cash_vm_received_eligible, initial_margin_posted) VALUES ('nsfr_22', '', NULL, 0, 0, FALSE, 0, 0, 0, 0, 0, 0, 0, 0);

-- Branch #23: nsfr/else/then => <expr>
INSERT INTO nettingset (id, counterparty, netting_agreement, collateral_received, required_from_counterparty, received_segregated, hqla_substitutable, due_not_called, downgrade_3_notches, posted_non_hqla1, received_reusable, variation_margin_posted, cash_vm_received_eligible, initial_margin_posted) VALUES ('nsfr_23', '', NULL, 0, 0, FALSE, 0, 0, 0, 0, 0, 0, 0, 0);

-- Branch #24: nsfr/else/else/then => assets(..)
INSERT INTO nettingset (id, counterparty, netting_agreement, collateral_received, required_from_counterparty, received_segregated, hqla_substitutable, due_not_called, downgrade_3_notches, posted_non_hqla1, received_reusable, variation_margin_posted, cash_vm_received_eligible, initial_margin_posted) VALUES ('nsfr_24', '', NULL, 0, 0, FALSE, 0, 0, 0, 0, 0, 0, 0, 0);

-- Branch #25: nsfr/else/else/else/then => <expr>
INSERT INTO nettingset (id, counterparty, netting_agreement, collateral_received, required_from_counterparty, received_segregated, hqla_substitutable, due_not_called, downgrade_3_notches, posted_non_hqla1, received_reusable, variation_margin_posted, cash_vm_received_eligible, initial_margin_posted) VALUES ('nsfr_25', '', NULL, 0, 0, FALSE, 0, 0, 0, 0, 0, 0, 0, 0);

-- Branch #26: nsfr/else/else/else/else/then => <expr>
INSERT INTO nettingset (id, counterparty, netting_agreement, collateral_received, required_from_counterparty, received_segregated, hqla_substitutable, due_not_called, downgrade_3_notches, posted_non_hqla1, received_reusable, variation_margin_posted, cash_vm_received_eligible, initial_margin_posted) VALUES ('nsfr_26', '', NULL, 0, 0, FALSE, 0, 0, 0, 0, 0, 0, 0, 0);

-- Branch #27: nsfr/else/else/else/else/else => <expr>
INSERT INTO nettingset (id, counterparty, netting_agreement, collateral_received, required_from_counterparty, received_segregated, hqla_substitutable, due_not_called, downgrade_3_notches, posted_non_hqla1, received_reusable, variation_margin_posted, cash_vm_received_eligible, initial_margin_posted) VALUES ('nsfr_27', '', NULL, 0, 0, FALSE, 0, 0, 0, 0, 0, 0, 0, 0);

-- Branch #28: cash_flows_in_currency/then => ccy_out(..)
INSERT INTO nettingset (id, counterparty, netting_agreement, collateral_received, required_from_counterparty, received_segregated, hqla_substitutable, due_not_called, downgrade_3_notches, posted_non_hqla1, received_reusable, variation_margin_posted, cash_vm_received_eligible, initial_margin_posted) VALUES ('cash_flows_in_currency_28', '', NULL, 0, 0, FALSE, 0, 0, 0, 0, 0, 0, 0, 0);

-- Branch #29: cash_flows_in_currency/else/then => ccy_in(..)
INSERT INTO nettingset (id, counterparty, netting_agreement, collateral_received, required_from_counterparty, received_segregated, hqla_substitutable, due_not_called, downgrade_3_notches, posted_non_hqla1, received_reusable, variation_margin_posted, cash_vm_received_eligible, initial_margin_posted) VALUES ('cash_flows_in_currency_29', '', NULL, 0, 0, FALSE, 0, 0, 0, 0, 0, 0, 0, 0);

-- Branch #30: cash_flows_in_currency/else/else => <expr>
INSERT INTO nettingset (id, counterparty, netting_agreement, collateral_received, required_from_counterparty, received_segregated, hqla_substitutable, due_not_called, downgrade_3_notches, posted_non_hqla1, received_reusable, variation_margin_posted, cash_vm_received_eligible, initial_margin_posted) VALUES ('cash_flows_in_currency_30', '', NULL, 0, 0, FALSE, 0, 0, 0, 0, 0, 0, 0, 0);

