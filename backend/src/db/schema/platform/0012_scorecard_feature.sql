-- The scorecard gets a switch of its own. At this release it starts on wherever the Timecard
-- page is, so no DSP stops collecting it; from then on the two switch apart. Its permissions
-- start with owners alone, as every feature's do.
INSERT OR IGNORE INTO dsp_features(dsp_id,feature,enabled,changed_at)
SELECT dsp_id,'scorecard',1,strftime('%Y-%m-%dT%H:%M:%fZ','now')
FROM dsp_features WHERE feature='timecard' AND enabled=1;
