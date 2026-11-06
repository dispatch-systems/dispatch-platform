use super::PendingMail;
use crate::{
    Error, Result, State,
    db::s,
    ensure,
    foundation::{config::Config, crypto},
};
use serde_json::json;
use std::{sync::Arc, time::Duration};
/// Messages sent per look at the outbox.
const BATCH: usize = 5;
pub async fn mailer(state: Arc<State>, mut stop: tokio::sync::watch::Receiver<bool>) {
    // The tick brings retries that have come due; a request that queues mail wakes it at once.
    let mut timer = tokio::time::interval(Duration::from_secs(5));
    let mut transport = None;
    loop {
        tokio::select! {
            _ = crate::cancelled(&mut stop) => break,
            _ = timer.tick() => {}
            _ = state.mail_wake.notified() => {}
        };
        if !state.config.mail_available() {
            continue;
        }
        if transport.is_none() {
            let config = state.config.clone();
            let result = tokio::task::spawn_blocking(move || MailTransport::new(&config))
                .await
                .unwrap_or_else(|_| Err(Error::new("email_transport_task_failed", 500)));
            match result {
                Ok(client) => {
                    super::transport_status(&state, None);
                    transport = Some(Arc::new(client));
                }
                Err(error) => {
                    super::transport_status(&state, Some(&error.code));
                    crate::foundation::observability::event(
                        "error",
                        "mail.transport_failed",
                        json!({"error":error.code,"mode":state.config.mail_mode}),
                    );
                    continue;
                }
            }
        }
        let pending = state.run_bookkeeping(|db| super::due(db, BATCH)).await;
        let rows = match pending {
            Ok(rows) => rows,
            Err(error) => {
                crate::foundation::observability::event(
                    "error",
                    "mail.queue_failed",
                    json!({"error":error.code}),
                );
                continue;
            }
        };
        // A full batch may have left mail behind: look again at once rather than on the tick.
        // Only when every delivery was recorded, since an unrecorded one is still pending and
        // would be sent again straight away.
        let mut again = rows.len() == BATCH;
        for row in rows {
            if *stop.borrow() {
                again = false;
                break;
            }
            // Its DSP disabled since, it waits until it's enabled; deleted, it's gone.
            let client = transport.as_ref().unwrap().clone();
            let message = row.clone();
            let delivered = match *client {
                MailTransport::Capture => state.read(move |db| super::capture(db, &message)).await,
                _ => send(&state, message, client).await,
            };
            let result = match delivered {
                Ok(Some(result)) => result,
                Ok(None) => continue,
                Err(error) => {
                    again = false;
                    crate::foundation::observability::event(
                        "error",
                        "mail.queue_failed",
                        json!({"error":error.code}),
                    );
                    continue;
                }
            };
            let error = result.err().map(|e| e.code);
            crate::foundation::observability::event(
                if error.is_some() { "warn" } else { "info" },
                "mail.delivery",
                json!({"mailId":row.id,"attempt":row.attempts+1,"error":error}),
            );
            if let Err(error) = state
                .run_bookkeeping(move |db| super::record_delivery(db, &row, error.as_deref()))
                .await
            {
                again = false;
                crate::foundation::observability::event(
                    "error",
                    "mail.record_failed",
                    json!({"error":error.code}),
                );
            }
        }
        if again {
            state.mail_wake.notify_one();
        }
    }
}
enum MailTransport {
    Capture,
    Cloudflare(reqwest::blocking::Client),
    Smtp(lettre::SmtpTransport),
}
impl MailTransport {
    fn new(config: &Config) -> Result<Self> {
        let error = || Error::new("email_transport_configuration_failed", 503);
        Ok(match config.mail_mode.as_str() {
            "capture" => Self::Capture,
            "cloudflare" => Self::Cloudflare(
                // HTTP/1.1 without compression, as mail has always been sent; only
                // Cortex reads use HTTP/2 and gzip.
                reqwest::blocking::Client::builder()
                    .http1_only()
                    .no_gzip()
                    .user_agent("Dispatch-Mail/1.0")
                    .timeout(Duration::from_secs(15))
                    .redirect(reqwest::redirect::Policy::none())
                    .build()
                    .map_err(|_| error())?,
            ),
            _ => Self::Smtp(
                lettre::SmtpTransport::from_url(config.smtp_url.as_deref().ok_or_else(error)?)
                    .map_err(|_| error())?
                    .timeout(Some(Duration::from_secs(15)))
                    .build(),
            ),
        })
    }
}
/// Sends the message `row` by `transport`, unless it no longer goes: none, or what became of
/// sending it. Development's captures are written by `capture`.
async fn send(
    state: &Arc<State>,
    row: PendingMail,
    transport: Arc<MailTransport>,
) -> Result<Option<Result<()>>> {
    let id = row.id.clone();
    if !state.read(move |db| super::sendable(db, &id)).await? {
        return Ok(None);
    }
    let (config, key) = (state.config.clone(), state.key.clone());
    let result =
        tokio::task::spawn_blocking(move || deliver(&config, &key, &row, &transport)).await;
    Ok(Some(result.unwrap_or_else(|_| {
        Err(Error::new("email_delivery_task_failed", 500))
    })))
}
fn deliver(
    config: &Config,
    key: &[u8],
    row: &PendingMail,
    transport: &MailTransport,
) -> Result<()> {
    let value = crypto::decrypt(key, &row.id, &row.encrypted_message)?;
    if let MailTransport::Cloudflare(client) = transport {
        ensure(
            s(&value, "environment") == config.environment.as_str()
                && s(&value, "origin") == config.origin,
            "email_environment_mismatch",
            503,
        )?;
        let error = || Error::new("email_delivery_failed", 503);
        let response = client
            .post(config.mail_worker_url.as_deref().ok_or_else(error)?)
            .bearer_auth(config.mail_worker_token.as_deref().ok_or_else(error)?)
            .json(&value)
            .send()
            .map_err(|e| {
                Error::new(
                    if e.is_timeout() {
                        "email_timeout"
                    } else if e.is_connect() {
                        "email_connection_failed"
                    } else {
                        "email_request_failed"
                    },
                    503,
                )
            })?;
        ensure(
            response.status().is_success(),
            &format!("email_http_{}", response.status().as_u16()),
            503,
        )?;
    } else {
        use lettre::{
            Transport,
            message::{MultiPart, SinglePart},
        };
        let error = || Error::new("email_delivery_failed", 503);
        let message = lettre::Message::builder()
            .from(
                config
                    .mail_from
                    .as_deref()
                    .ok_or_else(error)?
                    .parse()
                    .map_err(|_| error())?,
            )
            .to(s(&value, "to").parse().map_err(|_| error())?)
            .subject(s(&value, "subject"));
        let message = if let Some(html) = value["html"].as_str() {
            message.multipart(
                MultiPart::alternative()
                    .singlepart(SinglePart::plain(s(&value, "text").to_owned()))
                    .singlepart(SinglePart::html(html.to_owned())),
            )
        } else {
            message.body(s(&value, "text").to_owned())
        }
        .map_err(|_| error())?;
        let MailTransport::Smtp(transport) = transport else {
            unreachable!()
        };
        transport.send(&message).map_err(|e| {
            Error::new(
                if e.is_timeout() {
                    "email_timeout"
                } else if e.is_response() {
                    "email_smtp_rejected"
                } else {
                    "email_smtp_delivery_failed"
                },
                503,
            )
        })?;
    }
    Ok(())
}
