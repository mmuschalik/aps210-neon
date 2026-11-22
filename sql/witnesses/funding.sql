-- Z3-synthesized branch-covering witness dataset for module `FundingAndPlacements`
-- Every row below was solved by Z3 to satisfy a distinct leaf branch + domain constraints.

INSERT INTO fundingandplacements_params (mode, reporting_date) VALUES ('Conservative', '2026-09-30');

-- Branch #1: security_lcr_row/then => "none"
INSERT INTO issuedsecurity (id, kind, amount, maturity, offshore) VALUES ('security_lcr_row_1', 'ShortTermSecuritisation', 1, '2026-10-31', FALSE);

-- Branch #2: security_lcr_row/else/then => "12.1"
INSERT INTO issuedsecurity (id, kind, amount, maturity, offshore) VALUES ('security_lcr_row_2', 'CoveredBond', 1, '2025-09-30', FALSE);

-- Branch #3: security_lcr_row/else/else/then => "10.2"
INSERT INTO issuedsecurity (id, kind, amount, maturity, offshore) VALUES ('security_lcr_row_3', 'ShortTerm', 1, '2025-09-30', TRUE);

-- Branch #4: security_lcr_row/else/else/else => "10.1"
INSERT INTO issuedsecurity (id, kind, amount, maturity, offshore) VALUES ('security_lcr_row_4', 'ShortTerm', 1, '2025-09-30', FALSE);

-- Branch #5: capital_lcr_row/then => "10.1"
INSERT INTO capitalinstrument (id, tier, amount, maturity) VALUES ('capital_lcr_row_5', 'T2', 1, '2025-09-30');

-- Branch #6: capital_lcr_row/else => "none"
INSERT INTO capitalinstrument (id, tier, amount, maturity) VALUES ('capital_lcr_row_6', 'CET1', 1, NULL);

-- Branch #7: capital_nsfr_item/then => "12"
INSERT INTO capitalinstrument (id, tier, amount, maturity) VALUES ('capital_nsfr_item_7', 'T2', 1, '2025-09-30');

-- Branch #8: capital_nsfr_item/else => "1"
INSERT INTO capitalinstrument (id, tier, amount, maturity) VALUES ('capital_nsfr_item_8', 'CET1', 1, NULL);

-- Branch #9: capital_bucket/then => "b2"
INSERT INTO capitalinstrument (id, tier, amount, maturity) VALUES ('capital_bucket_9', 'CET1', 1, NULL);

-- Branch #10: capital_bucket/else/some => bucket(..)
INSERT INTO capitalinstrument (id, tier, amount, maturity) VALUES ('capital_bucket_10', 'T2', 1, '2025-09-30');

-- UNREACHABLE (unsat) Branch #11: capital_bucket/else/none => "b2"

-- Branch #12: placement_lcr_row/then => "none"
INSERT INTO placement (id, adi, amount, maturity, operational) VALUES ('placement_lcr_row_12', 'LcrAdi', 1, NULL, TRUE);

-- Branch #13: placement_lcr_row/else/then => "20.5"
INSERT INTO placement (id, adi, amount, maturity, operational) VALUES ('placement_lcr_row_13', 'LcrAdi', 1, NULL, FALSE);

-- Branch #14: placement_lcr_row/else/else => "none"
INSERT INTO placement (id, adi, amount, maturity, operational) VALUES ('placement_lcr_row_14', 'LcrAdi', 1, '2026-10-31', FALSE);

-- Branch #15: placement_nsfr_item/then => "33.1"
INSERT INTO placement (id, adi, amount, maturity, operational) VALUES ('placement_nsfr_item_15', 'LcrAdi', 1, NULL, TRUE);

-- Branch #16: placement_nsfr_item/else => "24.1"
INSERT INTO placement (id, adi, amount, maturity, operational) VALUES ('placement_nsfr_item_16', 'LcrAdi', 1, NULL, FALSE);

-- Branch #17: placement_bucket/some => bucket(..)
INSERT INTO placement (id, adi, amount, maturity, operational) VALUES ('placement_bucket_17', 'LcrAdi', 1, '2025-09-30', NULL);

-- Branch #18: placement_bucket/none => "b0"
INSERT INTO placement (id, adi, amount, maturity, operational) VALUES ('placement_bucket_18', 'LcrAdi', 1, NULL, NULL);

