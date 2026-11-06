//! Drive permissions and sharing refusals.
#[cfg(test)]
use super::fixture::fixture_sharing_changed;
use super::{ALL_DRIVES, fixture::fixture};
use crate::backend::google::{FILES, Google, HTTP, Timed, read, send, unreachable};
use dispatch_core::Result;
use serde::Deserialize;
use serde_json::json;

/// Someone a folder is shared with, as Drive lists them.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Share {
    pub id: String,
    /// `owner`, `writer`, `commenter` or `reader`.
    pub role: String,
    /// `user`, `group`, `domain` or `anyone`.
    #[serde(rename = "type")]
    pub kind: String,
    pub email_address: Option<String>,
}
/// Why Drive would not share with an address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// The address is no Google account, so the folder can't be shared with it.
    NoGoogleAccount,
    /// Drive refused for another reason, by its code: a Workspace that shares only inside
    /// itself, say.
    Other(String),
}

impl Google {
    /// Who the folder `id` is shared with.
    pub async fn shares(&self, access: &str, id: &str) -> Result<Vec<Share>> {
        if let Self::Fixture = self {
            return fixture(access, |drive| {
                let mut shares = vec![Share {
                    id: "owner".into(),
                    role: "owner".into(),
                    kind: "user".into(),
                    email_address: Some(drive.account.clone()),
                }];
                shares.extend(drive.shares.get(id).cloned().unwrap_or_default());
                Ok(shares)
            });
        }
        #[derive(Deserialize)]
        struct Shares {
            permissions: Vec<Share>,
        }
        let listed: Shares = send(
            HTTP.get(format!("{FILES}/{id}/permissions"))
                .bearer_auth(access)
                .query(&[
                    ("fields", "permissions(id,role,type,emailAddress)"),
                    ALL_DRIVES,
                ]),
        )
        .await?;
        Ok(listed.permissions)
    }
    /// Lets `email` edit the folder `id` and everything in it, without Google emailing them:
    /// the share's ID, or why Drive refused.
    pub async fn share(
        &self,
        access: &str,
        id: &str,
        email: &str,
    ) -> Result<std::result::Result<String, Refused>> {
        if let Self::Fixture = self {
            let shared = fixture(access, |drive| drive.share(id, email));
            #[cfg(test)]
            fixture_sharing_changed(id);
            return shared;
        }
        let response = HTTP
            .post(format!("{FILES}/{id}/permissions"))
            .bearer_auth(access)
            .query(&[
                ("sendNotificationEmail", "false"),
                ("fields", "id"),
                ALL_DRIVES,
            ])
            .json(&json!({"type": "user", "role": "writer", "emailAddress": email}))
            .timed()
            .await
            .map_err(unreachable)?;
        if response.status() == reqwest::StatusCode::BAD_REQUEST
            || response.status() == reqwest::StatusCode::FORBIDDEN
        {
            let body: serde_json::Value = response.json().await.unwrap_or_default();
            let reason = body
                .pointer("/error/errors/0/reason")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            return Ok(Err(match reason {
                "invalidSharingRequest" => Refused::NoGoogleAccount,
                other => Refused::Other(other.to_owned()),
            }));
        }
        #[derive(Deserialize)]
        struct Made {
            id: String,
        }
        let made: Made = read(response).await?;
        Ok(Ok(made.id))
    }
    /// Takes back the share `share` of the folder `id`. One already gone is taken back.
    pub async fn unshare(&self, access: &str, id: &str, share: &str) -> Result<()> {
        if let Self::Fixture = self {
            let unshared = fixture(access, |drive| {
                if let Some(shares) = drive.shares.get_mut(id) {
                    shares.retain(|each| each.id != share);
                }
                Ok(())
            });
            #[cfg(test)]
            fixture_sharing_changed(id);
            return unshared;
        }
        let response = HTTP
            .delete(format!("{FILES}/{id}/permissions/{share}"))
            .query(&[ALL_DRIVES])
            .bearer_auth(access)
            .timed()
            .await
            .map_err(unreachable)?;
        if response.status() == reqwest::StatusCode::NOT_FOUND || response.status().is_success() {
            return Ok(());
        }
        read::<serde_json::Value>(response).await.map(drop)
    }
}
