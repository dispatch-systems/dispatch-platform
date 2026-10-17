-- The links the meal-break page saved before Driver Match become Driver Match's own: an Amazon
-- ID nobody holds joins whoever holds the Paycom code it was linked to, and every link it saved
-- reads as someone's decision. No DSP kept anyone apart from Paycom there, so nothing else
-- carries over, and the page's setting goes.
INSERT OR IGNORE INTO person_ids(source,external_id,code,linked_by,linked_at)
    SELECT 'amazon', json_extract(link.value, '$.cortexId'), paycom.code, 'person',
           strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    FROM settings, json_each(settings.value, '$.links') AS link
    JOIN person_ids paycom ON paycom.source = 'paycom'
        AND paycom.external_id = json_extract(link.value, '$.paycomCode')
    WHERE settings.key = 'employees.provider_links'
        AND json_extract(link.value, '$.cortexId') IS NOT NULL;
UPDATE person_ids SET linked_by = 'person' WHERE linked_by = 'saved';
DELETE FROM settings WHERE key = 'employees.provider_links';
