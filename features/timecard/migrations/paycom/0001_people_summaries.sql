-- Derived people data; original publications remain unchanged for rollback and history.
CREATE TABLE IF NOT EXISTS paycom_people (
    code TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    department TEXT,
    position TEXT,
    first_seen TEXT,
    last_seen TEXT,
    listed INTEGER NOT NULL CHECK(listed IN (0,1))
);
CREATE TABLE IF NOT EXISTS paycom_people_workdays (
    employee_code TEXT NOT NULL,
    date TEXT NOT NULL,
    PRIMARY KEY(employee_code,date)
);
CREATE TABLE IF NOT EXISTS paycom_people_collected_days (date TEXT PRIMARY KEY);
CREATE TABLE IF NOT EXISTS paycom_people_state (
    id INTEGER PRIMARY KEY CHECK(id=1),
    dirty INTEGER NOT NULL CHECK(dirty IN (0,1))
);
INSERT OR IGNORE INTO paycom_people_state VALUES (1,1);
CREATE TRIGGER IF NOT EXISTS paycom_people_publications_insert
AFTER INSERT ON publications
WHEN (SELECT dirty FROM paycom_people_state WHERE id=1)=0
BEGIN
    UPDATE paycom_people_state SET dirty=1 WHERE id=1;
END;
CREATE TRIGGER IF NOT EXISTS paycom_people_publications_update
AFTER UPDATE ON publications
WHEN (SELECT dirty FROM paycom_people_state WHERE id=1)=0
BEGIN
    UPDATE paycom_people_state SET dirty=1 WHERE id=1;
END;
CREATE TRIGGER IF NOT EXISTS paycom_people_publications_delete
AFTER DELETE ON publications
WHEN (SELECT dirty FROM paycom_people_state WHERE id=1)=0
BEGIN
    UPDATE paycom_people_state SET dirty=1 WHERE id=1;
END;
CREATE TRIGGER IF NOT EXISTS paycom_people_employees_insert
AFTER INSERT ON employees
WHEN (SELECT dirty FROM paycom_people_state WHERE id=1)=0
BEGIN
    UPDATE paycom_people_state SET dirty=1 WHERE id=1;
END;
CREATE TRIGGER IF NOT EXISTS paycom_people_employees_update
AFTER UPDATE ON employees
WHEN (SELECT dirty FROM paycom_people_state WHERE id=1)=0
BEGIN
    UPDATE paycom_people_state SET dirty=1 WHERE id=1;
END;
CREATE TRIGGER IF NOT EXISTS paycom_people_employees_delete
AFTER DELETE ON employees
WHEN (SELECT dirty FROM paycom_people_state WHERE id=1)=0
BEGIN
    UPDATE paycom_people_state SET dirty=1 WHERE id=1;
END;
CREATE TRIGGER IF NOT EXISTS paycom_people_timecards_insert
AFTER INSERT ON timecards
WHEN (SELECT dirty FROM paycom_people_state WHERE id=1)=0
BEGIN
    UPDATE paycom_people_state SET dirty=1 WHERE id=1;
END;
CREATE TRIGGER IF NOT EXISTS paycom_people_timecards_update
AFTER UPDATE ON timecards
WHEN (SELECT dirty FROM paycom_people_state WHERE id=1)=0
BEGIN
    UPDATE paycom_people_state SET dirty=1 WHERE id=1;
END;
CREATE TRIGGER IF NOT EXISTS paycom_people_timecards_delete
AFTER DELETE ON timecards
WHEN (SELECT dirty FROM paycom_people_state WHERE id=1)=0
BEGIN
    UPDATE paycom_people_state SET dirty=1 WHERE id=1;
END;
