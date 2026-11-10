use crate::{Result, config::Origin, require};
use dispatch_core::support::api::types::{SupportBrowserGrant, SupportEvidence, SupportJob};
use reqwest::Client;
use serde_json::{Value, json};

#[derive(Clone)]
pub struct Platform {
    client: Client,
    pub origin: String,
    pub environment: String,
    token: String,
}

impl Platform {
    pub fn new(origin: &Origin) -> Result<Self> {
        let token = String::from_utf8(crate::config::read_private(&origin.token_file)?)?
            .trim()
            .to_owned();
        require(
            token.len() >= 32,
            "Support worker token is missing or too short",
        )?;
        Ok(Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            origin: origin.url.trim_end_matches('/').to_owned(),
            environment: origin.environment.clone(),
            token,
        })
    }

    pub async fn claim(&self) -> Result<Option<SupportJob>> {
        let value = self
            .post("/api/v1/support/worker/claim", &json!({}))
            .await?;
        Ok(serde_json::from_value(value)?)
    }

    pub async fn update(&self, job: &SupportJob, action: &str, mut body: Value) -> Result<Value> {
        body["lease"] = job.lease.clone().into();
        self.post(
            &format!("/api/v1/support/worker/jobs/{}/{action}", job.id),
            &body,
        )
        .await
    }

    /// Records one piece of evidence on the job; the platform gives it its id and time.
    pub async fn evidence(&self, job: &SupportJob, evidence: &SupportEvidence) -> Result<()> {
        self.update(job, "evidence", json!({"evidence":evidence}))
            .await?;
        Ok(())
    }

    pub async fn metadata(&self, job: &SupportJob) -> Result<SupportBrowserGrant> {
        let value = self.update(job, "browser", json!({})).await?;
        serde_json::from_value(value).map_err(|_| "Invalid read-only browser grant".into())
    }

    pub async fn deployment(&self) -> Result<Value> {
        let response = self
            .client
            .get(format!("{}/api/health", self.origin))
            .send()
            .await?;
        require(
            response.status().is_success(),
            "Support deployment metadata unavailable",
        )?;
        let result: Value = response.json().await?;
        Ok(
            json!({"origin":self.origin,"sourceSha":result["source"]["commit"],"version":result["source"]["version"],"release":result["release"]}),
        )
    }

    pub async fn diagnostics(&self, job: &SupportJob) -> Result<Value> {
        let response = self.leased(job, "diagnostics").await?;
        require(
            response.status().is_success(),
            "Support diagnostics unavailable",
        )?;
        Ok(response.json().await?)
    }

    pub async fn image(&self, job: &SupportJob, image: &str) -> Result<(String, Vec<u8>)> {
        require(
            image.starts_with("support_image_")
                && image.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
            "Invalid support image id",
        )?;
        let response = self.leased(job, &format!("attachments/{image}")).await?;
        require(
            response.status().is_success(),
            "Support report screenshot unavailable",
        )?;
        let kind = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .ok_or("Support screenshot media type missing")?
            .to_owned();
        require(
            matches!(
                kind.as_str(),
                "image/png" | "image/jpeg" | "image/webp" | "image/gif"
            ),
            "Unsupported report screenshot format",
        )?;
        Ok((kind, response.bytes().await?.to_vec()))
    }

    /// A GET of one of the job's own resources, which its lease authorizes.
    async fn leased(&self, job: &SupportJob, resource: &str) -> Result<reqwest::Response> {
        Ok(self
            .client
            .get(format!(
                "{}/api/v1/support/worker/jobs/{}/{resource}",
                self.origin, job.id
            ))
            .bearer_auth(&self.token)
            .header("x-support-lease", &job.lease)
            .send()
            .await?)
    }

    async fn post(&self, path: &str, body: &Value) -> Result<Value> {
        let response = self
            .client
            .post(format!("{}{path}", self.origin))
            .bearer_auth(&self.token)
            .json(body)
            .send()
            .await?;
        let status = response.status();
        let value: Value = response.json().await?;
        if !status.is_success() {
            // Server's structured refusal only: never emit HTTP headers, bearer tokens or full reports.
            return Err(format!(
                "Support API {}: {}",
                status.as_u16(),
                value["error"].as_str().unwrap_or("request refused")
            )
            .into());
        }
        Ok(value)
    }

    pub async fn upload(&self, job: &SupportJob, name: &str, bytes: Vec<u8>) -> Result<String> {
        let response = self
            .client
            .post(format!(
                "{}/api/v1/support/worker/jobs/{}/attachments?name={}",
                self.origin,
                job.id,
                url::form_urlencoded::byte_serialize(name.as_bytes()).collect::<String>()
            ))
            .bearer_auth(&self.token)
            .header("Content-Type", "application/octet-stream")
            .header("x-support-lease", &job.lease)
            .body(bytes)
            .send()
            .await?;
        require(
            response.status().is_success(),
            "Support evidence upload refused",
        )?;
        let result: Value = response.json().await?;
        Ok(result["id"]
            .as_str()
            .ok_or("Evidence attachment id missing")?
            .to_owned())
    }
}
