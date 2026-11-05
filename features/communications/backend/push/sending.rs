//! Each push: what it says (`message`), as the browser's service worker shows it, and sending it
//! to each browser, a few at once across the server, each within ten seconds, one that fails
//! tried once more a few seconds later. A browser its push service no longer knows (404, 410) is
//! let go of. Fixture mode sends none: its push services are `fixture`'s.
use super::{
    fixture,
    storage::{PushStore, Subscription},
    webpush::{self, Vapid},
};
use crate::api::types::{Message, WaitingMessage};
use dispatch_core::{Result, State, db::now, foundation::observability};
use futures_util::{StreamExt, stream};
use serde_json::json;
use std::{
    sync::{Arc, LazyLock},
    time::Duration,
};
use tokio::sync::Semaphore;

/// How many pushes go at once, of every DSP's batches together.
pub const AT_ONCE: usize = 8;
static TURNS: Semaphore = Semaphore::const_new(AT_ONCE);
/// How long a push service has to answer.
const TIMEOUT: Duration = Duration::from_secs(10);
/// How long after a push fails it's tried again, once.
const AGAIN: Duration = Duration::from_secs(5);
/// How long a push service holds a push for a browser that's away, in seconds: an hour, after
/// which it's no longer news.
const KEPT: u32 = 60 * 60;
/// The most characters of who wrote it, and of what they wrote, a push carries: within the 4 KB
/// a push holds.
const TITLE_MOST: usize = 100;
const BODY_MOST: usize = 300;

static HTTP: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("the TLS backend is built in")
});

/// What a push service answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    Sent,
    /// The browser let go of it, or it was never known: it's let go of here too.
    Gone,
    /// Refused or unanswered, with the status, if any.
    Failed(Option<u16>),
}

/// What a push of `waiting`, in the DSP `dsp`, says: titled with who wrote it, saying what as the
/// list shows it, opening the conversation at `address` (the DSP's page, up to its name), and
/// tagged with the message, so the page's own notification of it takes its place rather than
/// showing twice. The DSP, conversation and group it's of are for a page in front to say whether
/// it alerts of it itself.
pub fn message(dsp: &str, waiting: &WaitingMessage, address: &str) -> Vec<u8> {
    let opens = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("open", &waiting.conversation_id)
        .finish();
    json!({
        "title": cut(&waiting.from, TITLE_MOST),
        "body": cut(&preview(&waiting.message), BODY_MOST),
        "url": format!("{address}communications?{opens}"),
        "tag": waiting.message.id,
        "dsp": dsp,
        "conversation": waiting.conversation_id,
        "group": waiting.group_id,
    })
    .to_string()
    .into_bytes()
}

/// A message as a row of the list shows it: its text, else what it holds.
fn preview(message: &Message) -> String {
    if !message.text.trim().is_empty() {
        return message.text.clone();
    }
    let Some(first) = message.files.first() else {
        return String::new();
    };
    let photo = |mime: &str| {
        ["png", "jpeg", "gif", "webp", "avif"]
            .iter()
            .any(|kind| mime == format!("image/{kind}"))
    };
    match message.files.len() {
        _ if !message.files.iter().all(|file| photo(&file.mime)) => first.name.clone(),
        1 => "Photo".to_owned(),
        photos => format!("{photos} photos"),
    }
}

/// `text` up to `most` characters, an ellipsis ending one cut short.
fn cut(text: &str, most: usize) -> String {
    match text.char_indices().nth(most) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_owned(),
    }
}

/// Sends each of `pushes` to its browser, letting go of those gone; a failure is logged.
pub async fn send(
    state: &Arc<State>,
    dsp: &str,
    pushes: Vec<(Subscription, Vec<u8>)>,
) -> Result<()> {
    let Some(vapid) = webpush::vapid(&state.config) else {
        return Ok(());
    };
    let fixture = state.config.fixture;
    let answers: Vec<(String, Answer)> = stream::iter(pushes)
        .map(|(subscription, message)| async move {
            let mut answer = deliver(vapid, fixture, &subscription, &message).await;
            if matches!(answer, Answer::Failed(_)) {
                tokio::time::sleep(AGAIN).await;
                answer = deliver(vapid, fixture, &subscription, &message).await;
            }
            (subscription.endpoint, answer)
        })
        .buffer_unordered(AT_ONCE)
        .collect()
        .await;
    let mut gone = Vec::new();
    for (endpoint, answer) in answers {
        match answer {
            Answer::Sent => {}
            Answer::Gone => gone.push(endpoint),
            Answer::Failed(status) => observability::event(
                "warn",
                "communications_push_refused",
                json!({"dspId": dsp, "status": status}),
            ),
        }
    }
    if !gone.is_empty() {
        let at = dsp.to_owned();
        state
            .run_bookkeeping(move |db| db.drop_communications_pushes(&at, &gone))
            .await?;
    }
    Ok(())
}

/// Sends `message` to the browser `subscription` names, encrypted for it and signed by `vapid`,
/// in its turn among every push going.
async fn deliver(
    vapid: &Vapid,
    fixture: bool,
    subscription: &Subscription,
    message: &[u8],
) -> Answer {
    let Ok(_turn) = TURNS.acquire().await else {
        return Answer::Failed(None);
    };
    if fixture {
        return fixture::deliver(&subscription.endpoint, message).await;
    }
    // Both were checked as it was kept: either failing now, it can't be pushed to.
    let (Ok(endpoint), Ok(body)) = (
        url::Url::parse(&subscription.endpoint),
        webpush::encrypt(message, &subscription.p256dh, &subscription.auth),
    ) else {
        return Answer::Gone;
    };
    let sent = HTTP
        .post(endpoint.clone())
        .header(
            "Authorization",
            vapid.authorization(&endpoint, now() / 1000),
        )
        .header("Content-Encoding", "aes128gcm")
        .header("Content-Type", "application/octet-stream")
        .header("TTL", KEPT)
        .header("Urgency", "high")
        .body(body)
        .send()
        .await;
    match sent.map(|answer| answer.status().as_u16()) {
        Ok(status) if (200..300).contains(&status) => Answer::Sent,
        Ok(404 | 410) => Answer::Gone,
        Ok(status) => Answer::Failed(Some(status)),
        Err(_) => Answer::Failed(None),
    }
}
