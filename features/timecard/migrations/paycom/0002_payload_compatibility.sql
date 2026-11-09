-- Sidecars keep every legacy column and positional INSERT compatible with rollback.
CREATE TABLE IF NOT EXISTS paycom_payloads (
    format TEXT NOT NULL CHECK(length(format) BETWEEN 1 AND 80),
    digest TEXT NOT NULL CHECK(length(digest)=64 AND digest NOT GLOB '*[^0-9a-f]*'),
    body TEXT NOT NULL CHECK(json_valid(body)),
    PRIMARY KEY(format,digest),
    CHECK(format<>'paycom.punches.v1' OR json_type(body)='array'),
    CHECK(format<>'paycom.employee-capture.v1' OR json_type(body)='object')
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS paycom_timecard_payload_refs (
    publication_id TEXT NOT NULL,
    employee_code TEXT NOT NULL,
    date TEXT NOT NULL,
    format TEXT NOT NULL DEFAULT 'paycom.punches.v1' CHECK(format='paycom.punches.v1'),
    digest TEXT NOT NULL,
    PRIMARY KEY(publication_id,employee_code,date),
    FOREIGN KEY(publication_id,employee_code,date)
        REFERENCES timecards(publication_id,employee_code,date)
        ON UPDATE CASCADE ON DELETE CASCADE,
    FOREIGN KEY(format,digest) REFERENCES paycom_payloads(format,digest) ON DELETE RESTRICT
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS paycom_timecard_payload_refs_by_digest
    ON paycom_timecard_payload_refs(format,digest);

CREATE TABLE IF NOT EXISTS paycom_employee_sync_payload_refs (
    employee_code TEXT NOT NULL,
    period_from TEXT NOT NULL,
    period_to TEXT NOT NULL,
    format TEXT NOT NULL DEFAULT 'paycom.employee-capture.v1'
        CHECK(format='paycom.employee-capture.v1'),
    digest TEXT NOT NULL,
    payload_collected_at TEXT NOT NULL,
    PRIMARY KEY(employee_code,period_from,period_to),
    FOREIGN KEY(employee_code,period_from,period_to)
        REFERENCES employee_timecard_syncs(employee_code,period_from,period_to)
        ON UPDATE CASCADE ON DELETE CASCADE,
    FOREIGN KEY(format,digest) REFERENCES paycom_payloads(format,digest) ON DELETE RESTRICT
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS paycom_employee_sync_payload_refs_by_digest
    ON paycom_employee_sync_payload_refs(format,digest);

CREATE TRIGGER IF NOT EXISTS paycom_payloads_immutable BEFORE UPDATE ON paycom_payloads
BEGIN
    SELECT RAISE(ABORT,'immutable_paycom_payload');
END;

-- Both generations invalidate first. The new writer reattaches after its legacy write,
-- in the same transaction. INSERT handles REPLACE even with recursive_triggers disabled.
CREATE TRIGGER IF NOT EXISTS paycom_payload_timecards_insert AFTER INSERT ON timecards
BEGIN
    DELETE FROM paycom_timecard_payload_refs WHERE publication_id=NEW.publication_id
        AND employee_code=NEW.employee_code AND date=NEW.date;
END;
CREATE TRIGGER IF NOT EXISTS paycom_payload_timecards_update
AFTER UPDATE OF publication_id,employee_code,date,punches ON timecards
BEGIN
    DELETE FROM paycom_timecard_payload_refs
    WHERE (publication_id=OLD.publication_id AND employee_code=OLD.employee_code AND date=OLD.date)
       OR (publication_id=NEW.publication_id AND employee_code=NEW.employee_code AND date=NEW.date);
END;
CREATE TRIGGER IF NOT EXISTS paycom_payload_timecards_delete AFTER DELETE ON timecards
BEGIN
    DELETE FROM paycom_timecard_payload_refs WHERE publication_id=OLD.publication_id
        AND employee_code=OLD.employee_code AND date=OLD.date;
END;

CREATE TRIGGER IF NOT EXISTS paycom_payload_syncs_insert AFTER INSERT ON employee_timecard_syncs
BEGIN
    DELETE FROM paycom_employee_sync_payload_refs WHERE employee_code=NEW.employee_code
        AND period_from=NEW.period_from AND period_to=NEW.period_to;
END;
CREATE TRIGGER IF NOT EXISTS paycom_payload_syncs_update AFTER UPDATE ON employee_timecard_syncs
BEGIN
    DELETE FROM paycom_employee_sync_payload_refs
    WHERE (employee_code=OLD.employee_code AND period_from=OLD.period_from AND period_to=OLD.period_to)
       OR (employee_code=NEW.employee_code AND period_from=NEW.period_from AND period_to=NEW.period_to);
END;
CREATE TRIGGER IF NOT EXISTS paycom_payload_syncs_delete AFTER DELETE ON employee_timecard_syncs
BEGIN
    DELETE FROM paycom_employee_sync_payload_refs WHERE employee_code=OLD.employee_code
        AND period_from=OLD.period_from AND period_to=OLD.period_to;
END;
