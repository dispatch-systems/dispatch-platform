//! What Conduct's endpoints answer with, and the writes they take. Each answer type is written
//! to TypeScript in `generated/`, which `client.ts` imports.
use dispatch_core::{
    Result,
    db::{FromRow, Row},
    ensure,
    foundation::validate as v,
    text_enum,
};
use serde::Serialize;
use serde_json::Value;

text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/conduct/api/generated/"))]
    /// A writeup's tier, lowest first: each category climbs them in turn.
    pub enum ConductTier { Verbal => "verbal", Written => "written", Final => "final", }
}
impl ConductTier {
    /// Every tier, lowest first.
    pub const ALL: [Self; 3] = [Self::Verbal, Self::Written, Self::Final];
}

text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/conduct/api/generated/"))]
    /// What a final writeup comes with, beyond the final warning itself.
    pub enum ConductFinalKind { Suspension => "suspension", Investigation => "investigation", }
}

/// A kind of writeup: the tier it starts at and the wording that fills a new one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/conduct/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct ConductCategory {
    pub id: String,
    pub name: String,
    pub start_tier: ConductTier,
    /// The wording of each section a new writeup starts with: `[…]` marks a blank to fill.
    pub statement: String,
    pub prior: String,
    pub plan: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub revision: i64,
    /// Retired: no new writeup takes it, and its writeups keep it.
    pub retired: bool,
}
impl FromRow for ConductCategory {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            name: row.get("name")?,
            start_tier: row.get("start_tier")?,
            statement: row.get("statement")?,
            prior: row.get("prior")?,
            plan: row.get("plan")?,
            revision: row.get("revision")?,
            retired: row.get("retired")?,
        })
    }
}

/// Someone a writeup may be for, or one is: everyone still at the DSP, and anyone since gone
/// who has a writeup.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/conduct/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct ConductPerson {
    /// Their code in the DSP's directory, or the code a writeup kept once nobody holds it.
    pub code: String,
    pub name: String,
    /// The version of their photo, which `/api/dsp/people/{code}/photo` serves.
    pub photo: Option<String>,
    /// Whether they have left the DSP, or the directory no longer knows them.
    pub left: bool,
}

/// A writeup as a list shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/conduct/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct ConductWriteupRow {
    pub id: String,
    /// Who it's for, by the code the directory knows them by now.
    pub person: String,
    /// Their name as the writeup printed it.
    pub person_name: String,
    pub category_id: String,
    pub tier: ConductTier,
    pub final_kind: Option<ConductFinalKind>,
    /// How long the suspension is, as written, for a final writeup with one.
    pub suspension: Option<String>,
    /// Its date, YYYY-MM-DD.
    pub written_on: String,
    pub supervisor_name: String,
    /// The day the person signed it, YYYY-MM-DD: none until someone marks it signed.
    pub signed_on: Option<String>,
    /// Whether a photo of the signed paper is kept.
    pub has_copy: bool,
    /// Its revision, which a change or signing names as what it read.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub revision: i64,
}
impl FromRow for ConductWriteupRow {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            person: row.get("person")?,
            person_name: row.get("person_name")?,
            category_id: row.get("category_id")?,
            tier: row.get("tier")?,
            final_kind: row.get("final_kind")?,
            suspension: row.get("suspension")?,
            written_on: row.get("written_on")?,
            supervisor_name: row.get("supervisor_name")?,
            signed_on: row.get("signed_on")?,
            has_copy: row.get("has_copy")?,
            revision: row.get("revision")?,
        })
    }
}

/// The highest tier someone holds in a category: their highest writeup in it among those that
/// still count toward the next tier.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/conduct/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct ConductStanding {
    pub person: String,
    pub category_id: String,
    pub tier: ConductTier,
    /// The writeup that holds it: the latest at that tier.
    pub writeup_id: String,
    pub written_on: String,
}

/// `GET /api/dsp/conduct`: everything the page draws.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/conduct/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct ConductOverview {
    /// Today where the DSP is, YYYY-MM-DD.
    pub today: String,
    /// The first day a writeup dated on or after still counts toward the next tier.
    pub counts_from: String,
    /// Every category, retired ones too, in the DSP's order.
    pub categories: Vec<ConductCategory>,
    /// Everyone a writeup may be for or is, by name.
    pub people: Vec<ConductPerson>,
    /// Every writeup, the newest first.
    pub writeups: Vec<ConductWriteupRow>,
    /// Each person's highest tier in each category they hold one in.
    pub standings: Vec<ConductStanding>,
}

/// A writeup whole, as it prints.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/conduct/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct ConductWriteup {
    #[serde(flatten)]
    #[cfg_attr(feature = "ts", ts(flatten))]
    pub row: ConductWriteupRow,
    pub statement: String,
    pub prior: String,
    pub plan: String,
    /// Who marked it signed.
    pub signed_by_name: Option<String>,
}
impl FromRow for ConductWriteup {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            row: ConductWriteupRow::from_row(row)?,
            statement: row.get("statement")?,
            prior: row.get("prior")?,
            plan: row.get("plan")?,
            signed_by_name: row.get("signed_by_name")?,
        })
    }
}

/// What a write to a writeup answers: the writeup as it is now, and the page's data with it.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/conduct/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct ConductSaved {
    pub writeup: ConductWriteup,
    pub overview: ConductOverview,
}

/// The most characters a section of a writeup or a category's wording holds.
const SECTION: usize = 4000;

/// A writeup as someone writes it, before it's kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteupInput {
    pub person: String,
    pub category: String,
    pub tier: ConductTier,
    pub final_kind: Option<ConductFinalKind>,
    pub suspension: Option<String>,
    pub written_on: String,
    pub statement: String,
    pub prior: String,
    pub plan: String,
    /// The revision it was read at, for an edit.
    pub revision: Option<i64>,
}
impl WriteupInput {
    pub fn parse(body: &Value, edit: bool) -> Result<Self> {
        let mut fields = vec![
            "person",
            "category",
            "tier",
            "finalKind",
            "suspension",
            "writtenOn",
            "statement",
            "prior",
            "plan",
        ];
        if edit {
            fields.push("revision");
        }
        v::fields(body, &fields)?;
        let tier = tier(body, "tier")?;
        let final_kind = match body.get("finalKind") {
            None | Some(Value::Null) => None,
            Some(_) => Some(
                ConductFinalKind::parse(v::choice(
                    body,
                    "finalKind",
                    &["suspension", "investigation"],
                )?)
                .ok_or_else(|| dispatch_core::Error::new("invalid_input", 400))?,
            ),
        };
        ensure(
            tier == ConductTier::Final || final_kind.is_none(),
            "invalid_input",
            400,
        )?;
        let suspension = match body.get("suspension") {
            None | Some(Value::Null) => None,
            Some(_) => Some(v::text(body, "suspension", 1, 40)?.trim().to_owned()),
        };
        ensure(
            (final_kind == Some(ConductFinalKind::Suspension))
                == suspension.as_deref().is_some_and(|s| !s.is_empty()),
            "conduct_suspension_required",
            400,
        )?;
        let written_on = v::text(body, "writtenOn", 10, 10)?.to_owned();
        v::date(&written_on)?;
        Ok(Self {
            person: v::text(body, "person", 1, 128)?.to_owned(),
            category: v::text(body, "category", 1, 64)?.to_owned(),
            tier,
            final_kind,
            suspension,
            written_on,
            statement: section(body, "statement")?,
            prior: section(body, "prior")?,
            plan: section(body, "plan")?,
            revision: if edit {
                Some(v::integer(body, "revision", 1, i64::MAX)?)
            } else {
                None
            },
        })
    }
}

/// A category as someone sets it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CategoryInput {
    pub name: String,
    pub start_tier: ConductTier,
    pub statement: String,
    pub prior: String,
    pub plan: String,
    /// The revision it was read at, for an edit.
    pub revision: Option<i64>,
}
impl CategoryInput {
    pub fn parse(body: &Value, edit: bool) -> Result<Self> {
        let mut fields = vec!["name", "startTier", "statement", "prior", "plan"];
        if edit {
            fields.push("revision");
        }
        v::fields(body, &fields)?;
        let name = v::text(body, "name", 1, 60)?.trim().to_owned();
        ensure(!name.is_empty(), "invalid_input", 400)?;
        Ok(Self {
            name,
            start_tier: tier(body, "startTier")?,
            statement: section(body, "statement")?,
            prior: section(body, "prior")?,
            plan: section(body, "plan")?,
            revision: if edit {
                Some(v::integer(body, "revision", 1, i64::MAX)?)
            } else {
                None
            },
        })
    }
}

fn tier(body: &Value, key: &str) -> Result<ConductTier> {
    ConductTier::parse(v::choice(body, key, &["verbal", "written", "final"])?)
        .ok_or_else(|| dispatch_core::Error::new("invalid_input", 400))
}
/// A section's wording, its ends trimmed: something, and no more than `SECTION` characters.
fn section(body: &Value, key: &str) -> Result<String> {
    let text = v::text(body, key, 1, SECTION)?.trim().to_owned();
    ensure(!text.is_empty(), "conduct_section_required", 400)?;
    Ok(text)
}
