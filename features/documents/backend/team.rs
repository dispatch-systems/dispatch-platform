//! Who on the team edits the DSP's Documents in Google. Everyone who holds Use Documents gets
//! the main folder shared with them, and through it everything inside: at the Google account
//! they linked, or else their Dispatch email. Google refuses an address that is no Google
//! account, so Dispatch emails that member, once, how to link one, and Documents asks them to
//! until they do. Members who leave, or no longer use Documents, lose the share Dispatch gave
//! them.
use super::{
    drive::{Refused, Share},
    files,
    google::Google,
    storage::{DocumentsStore, Person, Sharing},
};
use crate::api::types::{GoogleSignIn, MySharing, SharingState};
use dispatch_core::{
    Result, State,
    accounts::Context,
    db::{Store, iso},
    ensure,
    foundation::{crypto, observability},
    server::{http::route::Dsp, mail::templates},
};
use serde_json::json;
use std::{
    collections::BTreeMap,
    sync::{Arc, LazyLock, Mutex},
};

/// The kind of email Dispatch sends a member who needs a Google account.
pub const GOOGLE_ACCOUNT_MAIL: &str = "documents.google_account";

/// A member who should edit in Google: they hold Use Documents.
struct Member {
    user: String,
    email: String,
}
fn members(db: &Store, dsp: &str) -> Result<Vec<Member>> {
    let mut wanted = Vec::new();
    for member in db.members(dsp)? {
        let holds = member.owner
            || db.grant(&member.user_id, dsp)?.is_some_and(|grant| {
                grant.owner || grant.permissions.iter().any(|p| p == "documents.use")
            });
        if holds {
            wanted.push(Member {
                user: member.user_id,
                email: member.email.to_lowercase(),
            });
        }
    }
    Ok(wanted)
}

/// Runs `call` on the DSP's Drive, once more with a fresh token if Google refused the one
/// kept, as `files` does.
macro_rules! on_drive {
    ($state:expr, $dsp:expr, $drive:ident, $call:expr) => {{
        let mut $drive = files::open($state, $dsp, false).await?;
        match $call.await {
            Err(error) if error.code == "google_connection_broken" => {
                files::forget($dsp);
                $drive = files::open($state, $dsp, true).await?;
                $call.await.map(|value| ($drive, value))
            }
            answer => answer.map(|value| ($drive, value)),
        }
    }};
}

/// Whether the team changed since the folder was last shared: someone joined or left, gained
/// or lost Use Documents, or is shared at an address they moved on from. Asks nothing of
/// Google, so it can be checked every minute.
pub fn stale(db: &Store, dsp: &str) -> Result<bool> {
    Ok(changed(&members(db, dsp)?, &db.documents_people(dsp)?))
}
fn changed(wanted: &[Member], people: &[Person]) -> bool {
    wanted.len() != people.len()
        || wanted.iter().any(|member| {
            people
                .iter()
                .find(|person| person.user == member.user)
                .is_none_or(|person| {
                    let target = person.linked.as_deref().unwrap_or(&member.email);
                    person
                        .shared
                        .as_ref()
                        .is_some_and(|(email, _)| !email.eq_ignore_ascii_case(target))
                })
        })
}

/// A turn at sharing each DSP's folder, so two syncs never share or email at once.
static TURNS: LazyLock<Mutex<BTreeMap<String, Arc<tokio::sync::Mutex<()>>>>> =
    LazyLock::new(Default::default);
fn turn(dsp: &str) -> Arc<tokio::sync::Mutex<()>> {
    TURNS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .entry(dsp.to_owned())
        .or_default()
        .clone()
}

/// Shares the main folder with everyone on the team who uses Documents, and takes it back
/// from those who left. Answers who the folder is shared with afterwards.
pub async fn sync(state: &Arc<State>, dsp: &str) -> Result<Vec<Share>> {
    let turn = turn(dsp);
    let _held = turn.lock().await;
    let (drive, mut shares) = on_drive!(
        state,
        dsp,
        drive,
        drive
            .google
            .shares(&drive.access, &drive.connection.folder_id)
    )?;
    let at = dsp.to_owned();
    let (wanted, people, name) = state
        .read(move |db| {
            Ok((
                members(db, &at)?,
                db.documents_people(&at)?,
                db.find_dsp(&at)?.name,
            ))
        })
        .await?;
    let folder = drive.connection.folder_id.clone();
    let shared_with = |shares: &[Share], email: &str| {
        shares
            .iter()
            .find(|share| {
                share.kind == "user"
                    && share
                        .email_address
                        .as_deref()
                        .is_some_and(|other| other.eq_ignore_ascii_case(email))
            })
            .map(|share| share.id.clone())
    };
    let mut changed: Vec<Person> = Vec::new();
    let mut mail: Vec<String> = Vec::new();
    for member in &wanted {
        let before = people.iter().find(|person| person.user == member.user);
        let linked = before.and_then(|person| person.linked.clone());
        let target = linked.clone().unwrap_or_else(|| member.email.clone());
        let mut person = before.cloned().unwrap_or(Person {
            user: member.user.clone(),
            linked: None,
            shared: None,
            sharing: Sharing::NeedsAccount,
            emailed_at: None,
        });
        if let Some(id) = shared_with(&shares, &target) {
            person.shared = Some((target, id));
            person.sharing = Sharing::Shared;
        } else {
            // A share at an address they moved on from goes.
            if let Some((_, old)) = person.shared.take() {
                drive.google.unshare(&drive.access, &folder, &old).await?;
                shares.retain(|share| share.id != old);
            }
            match drive.google.share(&drive.access, &folder, &target).await? {
                Ok(id) => {
                    shares.push(Share {
                        id: id.clone(),
                        role: "writer".into(),
                        kind: "user".into(),
                        email_address: Some(target.clone()),
                    });
                    person.shared = Some((target, id));
                    person.sharing = Sharing::Shared;
                }
                Err(Refused::NoGoogleAccount) => {
                    person.sharing = Sharing::NeedsAccount;
                    if person.emailed_at.is_none() {
                        mail.push(member.user.clone());
                        person.emailed_at = Some(iso());
                    }
                }
                Err(Refused::Other(code)) => person.sharing = Sharing::Refused(code),
            }
        }
        if before != Some(&person) {
            changed.push(person);
        }
    }
    let gone: Vec<Person> = people
        .into_iter()
        .filter(|person| !wanted.iter().any(|member| member.user == person.user))
        .collect();
    for person in &gone {
        if let Some((_, id)) = &person.shared {
            drive.google.unshare(&drive.access, &folder, id).await?;
            shares.retain(|share| &share.id != id);
        }
    }
    let at = dsp.to_owned();
    let origin = state.config.origin.clone();
    let dev = state.config.env().is_preview();
    state
        .run(move |db| {
            for person in &changed {
                db.save_documents_person(&at, person)?;
            }
            for person in &gone {
                db.remove_documents_person(&at, &person.user)?;
            }
            for user in &mail {
                email(db, &at, user, &name, &origin, dev);
            }
            Ok(())
        })
        .await?;
    Ok(shares)
}

/// Emails a member how to link a Google account, so the folder can be shared with them. A
/// message that can't be queued is noted in the log: sharing went on without it.
fn email(db: &Store, dsp: &str, user: &str, name: &str, origin: &str, dev: bool) {
    let url = format!("{origin}/#dsp/{dsp}/documents");
    let lines = [
        format!(
            "{name} keeps its team's folders, Docs and Sheets in Google Drive, through Dispatch."
        ),
        concat!(
            "Your Dispatch email isn't a Google account, so you can't open your team's ",
            "Documents yet."
        )
        .to_owned(),
        concat!(
            "Open Documents and link a Google account: any Gmail address works, or make a ",
            "free Google account with the email you already use. You keep signing in to ",
            "Dispatch as you do now."
        )
        .to_owned(),
    ];
    let paragraphs: Vec<&str> = lines.iter().map(String::as_str).collect();
    let subject = format!("Link a Google account to edit Documents at {name}");
    let mail = templates::notice(&templates::Notice {
        origin,
        dev,
        subject: &subject,
        preheader: "Link a Google account to edit your team's Docs and Sheets.",
        heading: "Link a Google account",
        paragraphs: &paragraphs,
        action: Some(("Open Documents", &url)),
        note: "It takes a minute. You only do it once.",
        footer: "You're getting this because your DSP uses Documents in Dispatch.",
    });
    if let Err(error) = db.email_member(dsp, user, GOOGLE_ACCOUNT_MAIL, &mail) {
        observability::event(
            "warn",
            "documents_mail_skipped",
            json!({"error": error.code}),
        );
    }
}

/// How the folder is shared with the member asking: pending until Documents first asks
/// Google, a moment after they're given Use Documents. The platform owner isn't one of the
/// DSP's members, so the folder is never shared with them.
pub fn mine(db: &Store, c: &Context) -> Result<Option<MySharing>> {
    if c.auth.user.platform_owner {
        return Ok(None);
    }
    let person = db
        .documents_people(&c.dsp.id)?
        .into_iter()
        .find(|person| person.user == c.actor());
    let Some(person) = person else {
        return Ok(Some(MySharing {
            state: SharingState::Pending,
            email: c.auth.user.email.clone(),
            linked: false,
        }));
    };
    Ok(Some(MySharing {
        state: match person.sharing {
            Sharing::Shared => SharingState::Shared,
            Sharing::NeedsAccount => SharingState::NeedsAccount,
            Sharing::Refused(_) => SharingState::Refused,
        },
        email: person
            .shared
            .map(|(email, _)| email)
            .or_else(|| person.linked.clone())
            .unwrap_or_else(|| c.auth.user.email.clone()),
        linked: person.linked.is_some(),
    }))
}

/// Shares the folder now, rather than within the minute, for a member Documents hasn't asked
/// Google about yet: unless it's already sharing for the DSP.
pub fn nudge(state: &Arc<State>, dsp: &str) {
    if turn(dsp).try_lock().is_err() {
        return;
    }
    let (state, dsp) = (Arc::clone(state), dsp.to_owned());
    tokio::spawn(async move {
        if let Err(error) = sync(&state, &dsp).await {
            observability::event(
                "warn",
                "documents_share_failed",
                json!({"error": error.code}),
            );
        }
    });
}

/// A link sign-in's state names the DSP, then says it links a member's own account, and
/// whether they started on Settings' Connections rather than Documents.
const LINK: &str = ".link.";
const FROM_SETTINGS: &str = ".link.settings.";
pub fn is_link(sign_in: &str) -> bool {
    sign_in.contains(LINK)
}
/// Whether Google sends the browser back to Documents: for a member who linked their account
/// from there. Every other sign-in started on Settings' Connections.
pub fn back_to_documents(sign_in: &str) -> bool {
    is_link(sign_in) && !sign_in.contains(FROM_SETTINGS)
}
/// Starts a member's sign-in with Google to link their own account, from Settings'
/// Connections or else from Documents.
pub fn start_link(db: &Store, c: &Context, from_settings: bool) -> Result<GoogleSignIn> {
    let google = Google::of(&db.config)?;
    let link = if from_settings { FROM_SETTINGS } else { LINK };
    let state = format!("{}{link}{}", c.dsp.id, crypto::token()?);
    let verifier = crypto::token()?;
    db.start_documents_sign_in(&c.dsp.id, c.actor(), &state, &verifier)?;
    Ok(GoogleSignIn {
        url: google.link_url(&db.config.origin, &state, &verifier),
    })
}
/// Finishes it: keeps the address Google proved, and shares the folder with it.
pub async fn finish_link(
    state: &Arc<State>,
    c: Context,
    access: Dsp,
    sign_in: String,
    code: String,
) -> Result<Option<MySharing>> {
    ensure(is_link(&sign_in), "documents_connect_expired", 409)?;
    let (dsp, actor) = (c.dsp.id.clone(), c.actor().to_owned());
    let verifier = state
        .run(move |db| db.finish_documents_sign_in(&dsp, &actor, &sign_in))
        .await?;
    let google = Google::of(&state.config)?;
    let email = google
        .linked_email(&state.config.origin, &code, &verifier)
        .await?;
    let c = state
        .run(move |db| {
            let c = access.revalidate(db, &c)?;
            db.link_documents_google(&c.dsp.id, c.actor(), &email)?;
            c.audit(db, "documents.linked", &email)?;
            Ok(c)
        })
        .await?;
    sync(state, &c.dsp.id).await?;
    state.read(move |db| mine(db, &c)).await
}

#[cfg(test)]
#[path = "../tests/backend/team.rs"]
mod tests;
