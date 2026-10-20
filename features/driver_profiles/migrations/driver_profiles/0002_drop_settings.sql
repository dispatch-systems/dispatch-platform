-- settings held only whether the DSP's first schedule was made, and Driver Profiles no longer
-- makes one: a DSP sets its own. No release reads it, since Driver Profiles first ships in the
-- one that drops it. A schedule made before is core's, in the DSP's database, and stays.
DROP TABLE settings;
