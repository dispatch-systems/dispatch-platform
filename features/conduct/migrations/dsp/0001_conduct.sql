-- What Conduct keeps for a DSP. Shipped migrations never change: add the next one instead.

-- The kinds of writeup, each with the tier it starts at and the wording that fills a new one.
-- A retired one stays for the writeups that name it.
CREATE TABLE IF NOT EXISTS conduct_categories (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    start_tier TEXT NOT NULL CHECK (start_tier IN ('verbal', 'written', 'final')),
    statement TEXT NOT NULL,
    prior TEXT NOT NULL,
    plan TEXT NOT NULL,
    position INTEGER NOT NULL,
    revision INTEGER NOT NULL DEFAULT 1,
    retired INTEGER NOT NULL DEFAULT 0 CHECK (retired IN (0, 1))
);
CREATE UNIQUE INDEX IF NOT EXISTS conduct_categories_name
    ON conduct_categories (name COLLATE NOCASE) WHERE retired = 0;

-- How long someone stays at each tier of a category before stepping down to a lower one, or to
-- a clean record (`to_tier` none). A tier without one never steps down.
CREATE TABLE IF NOT EXISTS conduct_step_downs (
    category_id TEXT NOT NULL REFERENCES conduct_categories(id),
    tier TEXT NOT NULL CHECK (tier IN ('verbal', 'written', 'final')),
    after INTEGER NOT NULL CHECK (after > 0),
    unit TEXT NOT NULL CHECK (unit IN ('days', 'weeks', 'months')),
    to_tier TEXT CHECK (
        to_tier IS NULL
        OR (tier = 'final' AND to_tier IN ('verbal', 'written'))
        OR (tier = 'written' AND to_tier = 'verbal')
    ),
    PRIMARY KEY (category_id, tier)
);

-- Each writeup, as it prints: who it's for by their directory code and their name as written,
-- and who wrote it. Once signed it never changes.
CREATE TABLE IF NOT EXISTS conduct_writeups (
    id TEXT PRIMARY KEY,
    person TEXT NOT NULL,
    person_name TEXT NOT NULL,
    category_id TEXT NOT NULL REFERENCES conduct_categories(id),
    tier TEXT NOT NULL CHECK (tier IN ('verbal', 'written', 'final')),
    final_kind TEXT CHECK (final_kind IN ('suspension', 'investigation')),
    suspension TEXT,
    written_on TEXT NOT NULL,
    supervisor_id TEXT NOT NULL,
    supervisor_name TEXT NOT NULL,
    statement TEXT NOT NULL,
    prior TEXT NOT NULL,
    plan TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    signed_on TEXT,
    signed_by_name TEXT,
    signed_at TEXT,
    CHECK (tier = 'final' OR final_kind IS NULL),
    CHECK ((final_kind = 'suspension') = (suspension IS NOT NULL))
);
CREATE INDEX IF NOT EXISTS conduct_writeups_written ON conduct_writeups (written_on DESC, created_at DESC);
CREATE INDEX IF NOT EXISTS conduct_writeups_person ON conduct_writeups (person, category_id);

-- A photo of a writeup's signed paper, kept with the DSP's data.
CREATE TABLE IF NOT EXISTS conduct_copies (
    writeup_id TEXT PRIMARY KEY REFERENCES conduct_writeups(id),
    media_type TEXT NOT NULL,
    bytes BLOB NOT NULL,
    added_by_name TEXT NOT NULL,
    added_at TEXT NOT NULL
);

-- The categories every DSP starts with, and their wording, which each DSP edits as its own.
INSERT OR IGNORE INTO conduct_categories (id, name, start_tier, statement, prior, plan, position) VALUES
('safety', 'Safety', 'verbal',
 'DA is in violation of safe driving practices by receiving a total of [number] Netradyne events, [events] on [date].',
 'DA is aware that safety and safe driving is always the most important as this has been discussed daily in stand up meetings and Amazon training. DA is aware they must drive extra cautious in the company vehicle following all road signs and laws, being aware of surroundings.',
 'DA understands safety is the number one priority while operating the company vehicle. DA understands they need to follow all road signs, laws and safe driving practices.',
 1),
('tardies', 'Excessive Tardies', 'verbal',
 'DA is in violation of work rules and standards by having excessive tardies on [dates].',
 'DA is aware they are supposed to be at work on scheduled work days on time. DA is aware of this procedure upon being hired.',
 'DA will arrive to work on time between 9:55AM and 10:00AM. DA will be on time for work to make sure there is adequate time to prepare for the day, attend SUM and do a proper DVIC. DA will call out by 8:55AM if they cannot make it to work.',
 2),
('absences', 'Unexcused Absences', 'verbal',
 'DA is in violation of work rules and standards by having excessive absences on [dates].',
 'DA is aware they are supposed to be at work on scheduled work days on time. DA is aware of this procedure upon being hired.',
 'DA is expected to come to work on scheduled shift days. DA will call out by 8:55AM if they cannot make it to work.',
 3),
('callout', 'Late Callout', 'verbal',
 'DA is in violation of work rules by calling off from work at [time] on [date].',
 'DA is aware they must call out from scheduled shifts a minimum of an hour prior to work start time. This is stated at start up meetings as well as in the company handbook.',
 'DA will inform dispatch by 8:55AM at the latest if they cannot make their scheduled shift. DA understands this is a work SOP and will follow company rules for call out procedure.',
 4),
('breaks', 'Breaks', 'verbal',
 'DA is in violation of work rules and standards regarding break procedure. DA took an additional [minutes] minutes [what happened]. No barriers were reported during this time.',
 'DA has been told about proper break procedure and attends SUM where this is discussed frequently. First 2 hours of work a 15 minute break, before the 5th hour of work a 30 minute lunch break (to be taken at the latest 2:45pm) and two hours after that a 15 minute break. These breaks are not to be combined or extended.',
 'DA will follow proper break procedure while on the clock and report to dispatch if unable to do so. DA understands buffer time for meal breaks only is a max of 5 mins. If a barrier occurs, DA is to report to dispatch at time of barrier.',
 5),
('timecard', 'Timecard Punches', 'verbal',
 'DA is in violation of work rules and standards for excessive missed/incorrect timecard punches. [dates and punches]',
 'DA is coached daily on timecard punches. DA is aware they must have 4 time punches each day by the end of their shift: In Day, Out Lunch, In Lunch and Out Day.',
 'DA will make sure all time card punches are updated/completed end of shift to avoid any discrepancies.',
 6),
('fuel', 'Pre-trip & Fuel', 'verbal',
 'DA is in violation of Company policy by not completing a proper pre-trip inspection. [what happened]',
 'DA is aware that inspections must be 100% in compliance along with proper fuel levels for the day, which is stated during daily SUMs, where we discuss gassing your vehicle in the morning, especially if you are around half a tank.',
 'DA understands to complete a full pre-trip and post-trip inspection and ensure they are fueled up for the day. DA will report any issues right away if there is a barrier to meet the requirements.',
 7),
('damage', 'Vehicle Damage', 'verbal',
 'DA is in violation of Company policy and safe driving practices by damaging [van] on route on [date]. Damage is confirmed by AVI photos. Damage is to [damage].',
 'DA attends daily SUM where safety is discussed about driving safe. DA attended Amazon training where GOAL “Get Out And Look” is discussed to always exit the vehicle to check van clearance if driver cannot visibly see.',
 'DA understands to GOAL “Get Out And Look” if they are unsure of the van clearance to clear. DA will be more aware of surroundings and report damage to dispatch as it occurs on route.',
 8),
('pod', 'POD', 'verbal',
 'DA had a Photo on Delivery score of [score]% in Week [week].',
 'DA attends daily start up meeting where delivery SOP is discussed. DA is aware they must take a clear photo on delivery with package clearly seen, no humans, not from in the van and not from too far away.',
 'DA will take better photos on delivery to improve their POD score moving forward.',
 9),
('cdf', 'CDF', 'verbal',
 'DA had [number] negative customer feedbacks in Week [week]; [reasons].',
 'DA attends daily SUM and Amazon training where following customer notes is discussed very thoroughly. DA is to follow notes exactly or bring package back.',
 'DA will follow all customer notes; if notes cannot be followed they will call dispatch for clarification or bring the package back not delivered. DA will double check to make sure they are at the correct location and handle packages with care.',
 10),
('uniform', 'Uniform', 'verbal',
 'DA did not arrive to work in full uniform. DA did not have on [missing item] on [date].',
 'DA is told upon being hired when you arrive to work you should be in complete uniform. This is also found in the employee handbook.',
 'DA will come to work in full uniform: Amazon branded polo, safety vest, hat, shorts or pants and the proper safety footwear.',
 11),
('oodt', 'OODT', 'verbal',
 'On [date], DA had [number] OODT (Out of Delivery Time) packages. DA did not report any barriers that would justify the return of these assigned route packages. The Company had to use additional resources to ensure the route was complete.',
 'DA attends morning start up meetings where reporting route barriers is discussed. Job description states that the employee complete the assigned route on their own.',
 'DA will maintain an adequate pace based on route volume and area in order to avoid OODT. DA will communicate with Dispatch any barriers that may impact the route being complete.',
 12);

-- Each starts by stepping down one tier at a time, six months at each.
INSERT OR IGNORE INTO conduct_step_downs (category_id, tier, after, unit, to_tier)
SELECT category.id, step.tier, 6, 'months', step.to_tier
FROM conduct_categories AS category
CROSS JOIN (
    SELECT 'final' AS tier, 'written' AS to_tier
    UNION ALL SELECT 'written', 'verbal'
    UNION ALL SELECT 'verbal', NULL
) AS step;
