use crate::{Result, jobs::Platform};
use dispatch_core::support::api::types::SupportJob;
use serde_json::{Value, json};
use std::sync::Mutex;

/// The platform stores this value opaquely; unknown fields survive worker updates.
pub(crate) struct Checkpoint(Mutex<Value>);

pub(crate) struct AnalysisProgress {
    pub stage: Option<String>,
    pub attempted: bool,
    pub usable: bool,
    pub browser_attempted: bool,
    pub live_browser_attempted: bool,
    pub live_browser_observed: bool,
}

/// Fields stay optional so legacy checkpoints retain their existing resume/refusal rules.
pub(crate) struct PublishedPr {
    pub queued: bool,
    pub head_sha: Option<String>,
    pub number: Option<u64>,
    pub url: Option<String>,
}

impl Checkpoint {
    pub fn new(value: Value) -> Self {
        Self(Mutex::new(value))
    }

    pub fn saved(&self) -> Value {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub fn analysis_progress(&self) -> AnalysisProgress {
        let value = self.saved();
        AnalysisProgress {
            stage: value["analysisStage"].as_str().map(str::to_owned),
            attempted: value["investigationAttempted"].as_bool().unwrap_or(false),
            usable: value["usableInvestigation"].as_bool().unwrap_or(false),
            browser_attempted: value["browserAttempted"].as_bool().unwrap_or(false),
            live_browser_attempted: value["liveBrowserAttempted"].as_bool().unwrap_or(false),
            live_browser_observed: value["liveBrowserObserved"].as_bool().unwrap_or(false),
        }
    }

    pub fn routing_decision(&self) -> Option<Value> {
        self.saved().get("routingDecision").cloned()
    }

    pub fn prior_analysis(&self) -> Option<Value> {
        self.saved().get("priorAnalysis").cloned()
    }

    pub fn published_pr(&self) -> PublishedPr {
        let value = self.saved();
        PublishedPr {
            queued: value["phase"] == "queued",
            head_sha: value["headSha"].as_str().map(str::to_owned),
            number: value["prNumber"].as_u64(),
            url: value["prUrl"].as_str().map(str::to_owned),
        }
    }

    pub fn failure(&self) -> String {
        self.saved()["failure"].as_str().unwrap_or("").to_owned()
    }

    pub fn last_handled_run(&self) -> u64 {
        self.saved()["lastHandledRun"].as_u64().unwrap_or(0)
    }

    fn merged(&self, fields: &Value) -> Value {
        let mut value = self.saved();
        if !value.is_object() {
            value = json!({});
        }
        if let Some(fields) = fields.as_object() {
            for (key, item) in fields {
                value[key] = item.clone();
            }
        }
        value
    }

    async fn publish(
        &self,
        platform: &Platform,
        job: &SupportJob,
        action: &str,
        mut body: Value,
        fields: Value,
    ) -> Result<()> {
        let value = self.merged(&fields);
        body["checkpoint"] = value.clone();
        platform.update(job, action, body).await?;
        // A refused publication must not advance the checkpoint read by the lease heartbeat.
        *self.0.lock().unwrap_or_else(|error| error.into_inner()) = value;
        Ok(())
    }

    pub async fn progress(
        &self,
        platform: &Platform,
        job: &SupportJob,
        status: &str,
        message: &str,
        fields: Value,
    ) -> Result<()> {
        self.publish(
            platform,
            job,
            "progress",
            json!({"status":status,"progress":message}),
            fields,
        )
        .await
    }

    pub async fn analysis(
        &self,
        platform: &Platform,
        job: &SupportJob,
        message: &str,
        fields: Value,
    ) -> Result<()> {
        if job.kind != "answer" {
            let status = if fields["phase"] == "routing" {
                "triaging"
            } else {
                "investigating"
            };
            return self.progress(platform, job, status, message, fields).await;
        }
        // Owner answers must preserve approval/queued status while selecting their tools.
        fields.as_object().ok_or("Invalid analysis checkpoint")?;
        self.publish(platform, job, "heartbeat", json!({}), fields)
            .await
    }

    pub async fn queue_pr(
        &self,
        platform: &Platform,
        job: &SupportJob,
        head: &str,
        number: u64,
        url: &str,
    ) -> Result<()> {
        self.progress(
            platform,
            job,
            "queued",
            "The verified change merges once its checks pass",
            json!({"phase":"queued","headSha":head,"prNumber":number,"prUrl":url,"failure":null}),
        )
        .await
    }
}

#[cfg(test)]
#[path = "../tests/backend/checkpoint.rs"]
mod tests;
