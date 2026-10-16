//! The links the meal-break page saved before Driver Match: read as decisions, and written
//! back as Driver Match decides, so the previous release still sees them after a rollback.
//! Remove once that release can no longer be rolled back to.
use dispatch_core::{
    Result,
    db::{Db, Store},
    foundation::crypto,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// Where the meal-break page kept its links.
const LINKS: &str = "employees.provider_links";
/// The sources the meal-break page's links name: they are the one place Driver Match names
/// a source.
pub(super) const PAYCOM: &str = "paycom";
pub(super) const AMAZON: &str = "amazon";

/// Links saved on the meal-break page before Driver Match: an Amazon ID with the Paycom
/// code someone linked it to, and Amazon IDs someone kept apart from Paycom.
#[derive(Clone, Debug, Default)]
pub(crate) struct Saved {
    pub links: BTreeMap<String, String>,
    pub separate: BTreeSet<String>,
}

/// The links saved on the meal-break page before Driver Match, read as decisions.
pub(super) fn saved_links(store: &Store, dsp: &str) -> Result<Saved> {
    let value = store.dsp(dsp)?.setting(LINKS, Value::Null)?;
    let text = |v: &Value| v.as_str().map(str::to_owned);
    Ok(Saved {
        links: value["links"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|l| Some((text(&l["cortexId"])?, text(&l["paycomCode"])?)))
            .collect(),
        separate: value["separate"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(text)
            .collect(),
    })
}

/// Writes the confirmed links back where the meal-break page kept them, so the previous
/// release still sees them after a rollback. Remove once that release can no longer be
/// rolled back to.
pub(super) fn mirror_links(db: &Db) -> Result<()> {
    let before = db.setting(LINKS, json!({"revision":0,"links":[]}))?;
    let mut taken = BTreeSet::new();
    let mut links = vec![];
    for (amazon, paycom) in db.query_as::<(String, String)>(
        "SELECT a.external_id,p.external_id FROM person_ids a JOIN person_ids p \
         ON p.code=a.code AND p.source='paycom' WHERE a.source='amazon' AND EXISTS \
         (SELECT 1 FROM person_ids c WHERE c.code=a.code AND c.linked_by IN ('saved','person')) \
         ORDER BY a.linked_at,a.external_id,p.linked_at",
        [],
    )? {
        // The previous release lets each Paycom code be linked once.
        if taken.insert(paycom.clone()) {
            links.push(json!({"id":crypto::id("employee")?,"cortexId":amazon,"paycomCode":paycom}));
        }
    }
    // A link saved for a driver whose data is no longer stored has no person to speak
    // for it, so it stays as it was saved.
    let known: BTreeSet<String> = db
        .query_as::<(String,)>(
            "SELECT external_id FROM person_ids WHERE source='amazon'",
            [],
        )?
        .into_iter()
        .map(|(id,)| id)
        .collect();
    for link in before["links"].as_array().into_iter().flatten() {
        if let (Some(amazon), Some(paycom)) =
            (link["cortexId"].as_str(), link["paycomCode"].as_str())
            && !known.contains(amazon)
            && taken.insert(paycom.to_owned())
        {
            links.push(link.clone());
        }
    }
    db.set(
        LINKS,
        &json!({
            "revision": before["revision"].as_i64().unwrap_or(0) + 1,
            "links": links,
            "separate": before["separate"].clone(),
        }),
    )
}
