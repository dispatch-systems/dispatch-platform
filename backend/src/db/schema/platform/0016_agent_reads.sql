-- What keys and apps read. agent_keys gained, in code before this runs, areas: the kinds of
-- data it reads at every DSP without settings of its own, comma-separated in the Agents
-- page's order (routes, locations, timecards, meal_breaks, dvic, feedback, safety, returns,
-- scorecard; empty for none), and bypass: whether it reads them even where a DSP has the
-- feature switched off. The old tools and locations columns stay for an older release; they
-- are written as tools 'full' and locations as whether areas has it, and no longer read.
-- agent_activity gained bypassed: whether the call read a switched-off feature that way.

-- agent_key_dsp_reads: a DSP's own settings for a key or app, read there in place of its own.
-- Only for a DSP the key reaches; they go when the key stops reaching it.
CREATE TABLE IF NOT EXISTS agent_key_dsp_reads (
    key_id TEXT NOT NULL REFERENCES agent_keys(id) ON DELETE CASCADE,
    dsp_id TEXT NOT NULL REFERENCES dsps(id),
    areas TEXT NOT NULL,
    bypass INTEGER NOT NULL CHECK(bypass IN (0,1)),
    PRIMARY KEY(key_id, dsp_id)
);

-- Keys and apps from before read every kind of data, delivery addresses as they had them.
-- The column's default is every kind but addresses.
UPDATE agent_keys
SET areas='routes,locations,timecards,meal_breaks,dvic,feedback,safety,returns,scorecard'
WHERE locations=1;
