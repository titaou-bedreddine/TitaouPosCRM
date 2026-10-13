-- Fix 003 — Cash register: settlement movements were written in CENTIMES by the
-- buggy deposit code (TrucksView sent tp.out_actual_cash raw). Local
-- cash_movements are DZD — convert those rows back to DZD and correct the
-- affected cash session's expected_cash by the same delta.
--
-- Before: a 3190 DA deposit wrote 319000 into cash_movements.amount and
-- +319000 into cash_sessions.expected_cash -> register showed "319000 DZD".
-- After:  3190 in both columns.

-- Record whether this run is the first (flag guard): a temp table captures the
-- pre-flag state inside this batch so the session delta is subtracted exactly once.
CREATE TEMP TABLE _m_flag_state AS
SELECT COUNT(*) AS had_flag FROM app_settings WHERE key = 'settlements_centimes_migrated_v1';

-- Set the flag before anything else so nothing re-runs on subsequent starts.
INSERT OR IGNORE INTO app_settings (key, value) VALUES ('settlements_centimes_migrated_v1', '1');

-- Correct the session's expected_cash by the same delta — only when this is the
-- very first run (had_flag = 0); per-session subquery keeps it atomic.
UPDATE cash_sessions
SET expected_cash = expected_cash - (
  SELECT SUM(m2.amount - m2.amount / 100)
  FROM cash_movements m2
  WHERE m2.session_id = cash_sessions.id
    AND m2.reference_type = 'truck_settlement'
)
WHERE (SELECT had_flag FROM _m_flag_state) = 0
  AND EXISTS (
    SELECT 1 FROM cash_movements
    WHERE session_id = cash_sessions.id AND reference_type = 'truck_settlement'
  );

-- Per-movement normalization — idempotent via the marker table so a second
-- migration run can never divide an already-DZD amount.
CREATE TABLE IF NOT EXISTS cash_settlement_dzd_migrated (movement_id INTEGER PRIMARY KEY);

INSERT INTO cash_settlement_dzd_migrated (movement_id)
SELECT id
FROM cash_movements
WHERE reference_type = 'truck_settlement'
  AND id NOT IN (SELECT movement_id FROM cash_settlement_dzd_migrated);

UPDATE cash_movements
SET amount = ROUND(amount / 100)
WHERE reference_type = 'truck_settlement'
  AND EXISTS (SELECT 1 FROM cash_settlement_dzd_migrated m WHERE m.movement_id = cash_movements.id);

DROP TABLE _m_flag_state;
