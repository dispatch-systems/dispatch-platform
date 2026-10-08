//! Who on the team edits the DSP's Documents in Google. Everyone who holds Use Documents gets
//! the main folder shared with them, and through it everything inside: at the Google account
//! they linked, or else their Dispatch email. Google refuses an address that is no Google
//! account, so Dispatch emails that member how to link one. Members who leave, or no longer
//! use Documents, lose the share Dispatch gave them. Anyone else the folder is shared with in
//! Google Drive is listed for those who manage Documents to remove.
use super::{
    drive::{Refused, Share},
    files::{self, Drive},
    google::Google,
    storage::{DocumentsStore, Person, Sharing},
};
use crate::api::types::{
    DocumentsTeam, GoogleSignIn, MySharing, SharingState, TeamOutsider, TeamPerson,
};
use dispatch_core::{
    Error, Result, State,
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
    name: String,
    email: String,
    owner: bool,
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
                name: member.name,
                email: member.email.to_lowercase(),
                owner: member.owner,
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
            "Your Dispatch email isn't a Google account, so you can see your team's files in ",
            "Dispatch but can't edit Docs and Sheets in Google yet."
        )
        .to_owned(),
        concat!(
            "To edit them, open Documents and link a Google account: any Gmail address works, ",
            "or make a free Google account with the email you already use."
        )
        .to_owned(),
    ];
    let paragraphs: Vec<&str> = lines.iter().map(String::as_str).collect();
    let subject = format!("Link a Google account to edit {name}'s Documents");
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
    if let Err(error) = db.email_member(user, GOOGLE_ACCOUNT_MAIL, &mail) {
        observability::event(
            "warn",
            "documents_mail_skipped",
            json!({"error": error.code}),
        );
    }
}

/// The team as the team access panel shows it, with who the folder is shared with now.
async fn view(
    state: &Arc<State>,
    dsp: &str,
    shares: Vec<Share>,
    drive: &Drive,
) -> Result<DocumentsTeam> {
    let storage = drive.google.storage(&drive.access).await?;
    let at = dsp.to_owned();
    let (wanted, people) = state
        .read(move |db| Ok((members(db, &at)?, db.documents_people(&at)?)))
        .await?;
    let mut listed: Vec<TeamPerson> = wanted
        .iter()
        .map(|member| {
            let person = people.iter().find(|person| person.user == member.user);
            let (state, refusal) = match person.map(|person| &person.sharing) {
                Some(Sharing::Shared) => (SharingState::Shared, None),
                Some(Sharing::Refused(code)) => (SharingState::Refused, Some(code.clone())),
                _ => (SharingState::NeedsAccount, None),
            };
            TeamPerson {
                user_id: member.user.clone(),
                name: member.name.clone(),
                email: person
                    .and_then(|person| person.linked.clone())
                    .unwrap_or_else(|| member.email.clone()),
                state,
                refusal,
                emailed_at: person.and_then(|person| person.emailed_at.clone()),
                owner: member.owner,
            }
        })
        .collect();
    listed.sort_by(|a, b| a.name.cmp(&b.name));
    let team: Vec<String> = people
        .iter()
        .filter_map(|person| {
            person
                .shared
                .as_ref()
                .map(|(email, _)| email.to_lowercase())
        })
        .chain(listed.iter().map(|person| person.email.to_lowercase()))
        .collect();
    let outsiders = shares
        .into_iter()
        .filter(|share| share.role != "owner")
        .filter_map(|share| {
            let email = share.email_address?;
            (!team.contains(&email.to_lowercase())).then_some(TeamOutsider {
                share_id: share.id,
                email,
            })
        })
        .collect();
    Ok(DocumentsTeam {
        people: listed,
        outsiders,
        storage_used: storage.used,
        storage_limit: storage.limit,
    })
}

/// The team, its sharing brought up to date first.
pub async fn team(state: &Arc<State>, c: &Context) -> Result<DocumentsTeam> {
    let shares = sync(state, &c.dsp.id).await?;
    let drive = files::open(state, &c.dsp.id, false).await?;
    view(state, &c.dsp.id, shares, &drive).await
}

/// Emails the member `user` again how to link a Google account.
pub async fn email_again(
    state: &Arc<State>,
    c: Context,
    access: Dsp,
    user: String,
) -> Result<DocumentsTeam> {
    let (origin, dev) = (state.config.origin.clone(), state.config.env().is_preview());
    let asking = c.clone();
    state
        .run(move |db| {
            let c = access.revalidate(db, &c)?;
            let dsp = c.dsp.id.clone();
            let mut person = db
                .documents_people(&dsp)?
                .into_iter()
                .find(|person| person.user == user && person.sharing == Sharing::NeedsAccount)
                .ok_or_else(|| Error::new("documents_person_not_found", 404))?;
            let name = db.find_dsp(&dsp)?.name;
            email(db, &dsp, &user, &name, &origin, dev);
            person.emailed_at = Some(iso());
            db.save_documents_person(&dsp, &person)?;
            Ok(())
        })
        .await?;
    team(state, &asking).await
}

/// Takes back a share someone made in Google Drive for someone not on the team.
pub async fn remove(
    state: &Arc<State>,
    c: Context,
    access: Dsp,
    share: String,
) -> Result<DocumentsTeam> {
    let dsp = c.dsp.id.clone();
    let shares = sync(state, &dsp).await?;
    let drive = files::open(state, &dsp, false).await?;
    let before = view(state, &dsp, shares, &drive).await?;
    let outsider = before
        .outsiders
        .iter()
        .find(|outsider| outsider.share_id == share)
        .ok_or_else(|| Error::new("documents_share_not_found", 404))?
        .email
        .clone();
    drive
        .google
        .unshare(&drive.access, &drive.connection.folder_id, &share)
        .await?;
    state
        .run(move |db| {
            let c = access.revalidate(db, &c)?;
            c.audit(db, "documents.unshared", &outsider)
        })
        .await?;
    let shares = drive
        .google
        .shares(&drive.access, &drive.connection.folder_id)
        .await?;
    view(state, &dsp, shares, &drive).await
}

/// How the folder is shared with the member asking, once Documents has tried.
pub fn mine(db: &Store, c: &Context) -> Result<Option<MySharing>> {
    let person = db
        .documents_people(&c.dsp.id)?
        .into_iter()
        .find(|person| person.user == c.actor());
    Ok(person.map(|person| MySharing {
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
/// How many on the team edit in Google.
pub fn editors(db: &Store, dsp: &str) -> Result<u32> {
    Ok(db
        .documents_people(dsp)?
        .iter()
        .filter(|person| person.sharing == Sharing::Shared)
        .count() as u32)
}

/// A link sign-in's state names the DSP, then says it links a member's own account.
const LINK: &str = ".link.";
pub fn is_link(sign_in: &str) -> bool {
    sign_in.contains(LINK)
}
/// Starts a member's sign-in with Google to link their own account.
pub fn start_link(db: &Store, c: &Context) -> Result<GoogleSignIn> {
    let google = Google::of(&db.config)?;
    let state = format!("{}{LINK}{}", c.dsp.id, crypto::token()?);
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
