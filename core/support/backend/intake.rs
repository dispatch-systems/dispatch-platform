use super::{
    api::types::{SupportIntake, SupportIntakeOutcome},
    conversations, decode, jobs, notifications, tickets,
};
use crate::{
    Result,
    db::{Store, now, s},
    ensure,
    foundation::validate as v,
};
use rusqlite::params;
use serde_json::{Value, json};

impl SupportIntakeOutcome {
    pub(crate) fn status(self) -> &'static str {
        match self {
            Self::Acknowledged => "acknowledged",
            Self::Answered => "answered",
            Self::UnclearBug => "unclear_bug",
            Self::UnclearFeature => "unclear_feature",
        }
    }
}

pub(crate) fn latest(db: &Store, ticket: &str, version: u32) -> Result<Option<SupportIntake>> {
    let checkpoints = db.platform.query_as::<(String,)>(
        "SELECT checkpoint FROM support_jobs WHERE ticket_id=? AND kind='investigate' \
         AND version=? AND status='done' ORDER BY updated_at DESC,rowid DESC LIMIT 20",
        params![ticket, version],
    )?;
    for (checkpoint,) in checkpoints {
        let checkpoint: Value = decode(&checkpoint)?;
        if let Some(intake) = checkpoint.get("intake") {
            return Ok(Some(serde_json::from_value(intake.clone())?));
        }
    }
    Ok(None)
}

pub(crate) fn complete(db: &Store, job: &str, lease: &str, result: &Value) -> Result<()> {
    let run = jobs::leased(db, job, lease)?;
    let ticket = tickets::record(db, &run.ticket)?;
    ensure(
        run.kind == "investigate"
            && ticket["proposal_version"] == run.version
            && ticket["approved_version"].is_null()
            && ticket["pr_url"].is_null()
            && ["triaging", "investigating", "needs_information"].contains(&s(&ticket, "status")),
        "support_transition_refused",
        409,
    )?;
    let mut intake: SupportIntake = serde_json::from_value(result.clone())?;
    intake.reply = v::text(result, "reply", 1, 40_000)?.trim().into();
    intake.reason = v::text(result, "reason", 1, 10_000)?.trim().into();
    ensure(
        !intake.reply.is_empty()
            && !intake.reason.is_empty()
            && intake.findings.len() <= 40
            && intake.questions.len() <= 10
            && intake
                .findings
                .iter()
                .chain(&intake.questions)
                .all(|text| !text.trim().is_empty() && text.len() <= 4_000),
        "invalid_input",
        400,
    )?;
    let (status, progress) = match intake.outcome {
        SupportIntakeOutcome::Acknowledged => ("acknowledged", "Your message was acknowledged"),
        SupportIntakeOutcome::Answered => ("answered", "Your question was answered"),
        SupportIntakeOutcome::UnclearBug => ("unclear_bug", "Unclear Bug Report"),
        SupportIntakeOutcome::UnclearFeature => ("unclear_feature", "Unclear Feature"),
    };
    if matches!(status, "unclear_bug" | "unclear_feature") {
        ensure(
            intake.investigation_attempted
                && !intake.findings.is_empty()
                && !intake.questions.is_empty()
                && s(&ticket, "kind")
                    == if status == "unclear_bug" {
                        "bug"
                    } else {
                        "feature"
                    },
            "support_intake_incomplete",
            400,
        )?;
    }
    let mut checkpoint = jobs::checkpoint(db, job, lease)?;
    let previous = if status == "acknowledged" {
        latest(db, &run.ticket, run.version)?.filter(|saved| {
            matches!(
                saved.outcome,
                SupportIntakeOutcome::UnclearBug | SupportIntakeOutcome::UnclearFeature
            )
        })
    } else {
        None
    };
    let (visible_status, visible_progress) =
        previous
            .as_ref()
            .map_or((status, progress), |saved| match saved.outcome {
                SupportIntakeOutcome::UnclearBug => ("unclear_bug", "Unclear Bug Report"),
                SupportIntakeOutcome::UnclearFeature => ("unclear_feature", "Unclear Feature"),
                _ => (status, progress),
            });
    checkpoint["phase"] = json!("intake_completed");
    checkpoint["intakeCompletion"] = serde_json::to_value(&intake)?;
    checkpoint["intake"] = serde_json::to_value(previous.as_ref().unwrap_or(&intake))?;
    db.platform.exec(
        "UPDATE support_jobs SET checkpoint=?,updated_at=? WHERE id=?",
        params![serde_json::to_string(&checkpoint)?, now(), job],
    )?;
    conversations::add(db, &run.ticket, "Hermes", "agent", &intake.reply, false)?;
    conversations::add(db, &run.ticket, "Hermes", "agent", &intake.reason, true)?;
    tickets::progress(db, &run.ticket, visible_status, visible_progress)?;
    notifications::reporter(db, &run.ticket, visible_progress)?;
    notifications::owners(db, &run.ticket, "AI Support intake review is ready")?;
    Ok(())
}
