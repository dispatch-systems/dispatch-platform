//! Who on the team edits the DSP's Documents in Google. Everyone who holds Use Documents gets
//! the main folder shared with them, and through it everything inside: at the Google account
//! they linked, or else their Dispatch email. Google refuses an address that is no Google
//! account, so Dispatch emails that member, once, how to link one, and Documents asks them to
//! until they do. Members who leave, or no longer use Documents, lose the share Dispatch gave
//! them. A file someone else owns, which Documents holds a shortcut to, is shared with them
//! too, as far as the account may share it.
use super::{
    access::{self, on_drive},
    drive::{Refused, Share},
    google::{self, Google},
    grant::{self, Grant},
    storage::{DocumentsStore, Person, SharedFile, Sharing},
};
use crate::api::types::{GoogleSignIn, MySharing, SharingState};
use dispatch_core::{
    Result, State,
    accounts::Context,
    db::{Store, iso},
    ensure,
    foundation::{config::Site, crypto, observability},
    server::{http::route::Dsp, mail::templates},
};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
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

/// Whether the team changed since the folder was last shared: someone joined or left, gained
/// or lost Use Documents, is shared at an address they moved on from, or isn't shared yet at
/// the one they moved to. Asks nothing of Google, so it can be checked every minute.
pub fn stale(db: &Store, dsp: &str) -> Result<bool> {
    Ok(changed(&members(db, dsp)?, &db.documents_people(dsp)?))
}
fn changed(wanted: &[Member], people: &[Person]) -> bool {
    wanted.len() != people.len()
        || wanted.iter().any(|member| {
            people
                .iter()
                .find(|person| person.user == member.user)
                .is_none_or(|person| match &person.shared {
                    Some((email, _)) => {
                        let target = person.linked.as_deref().unwrap_or(&member.email);
                        !email.eq_ignore_ascii_case(target)
                    }
                    None => moving(person),
                })
        })
}
/// Whether the member's share at an address they moved on from was taken back, and the
/// folder not yet shared at the new one: the DSP was disabled between the two.
fn moving(person: &Person) -> bool {
    person.sharing == Sharing::Shared && person.shared.is_none()
}

/// Captures the current grant once; queued callbacks use sync_for with their existing owner.
pub async fn sync(state: &Arc<State>, dsp: &str) -> Result<Vec<Share>> {
    let owner = grant::capture(state, dsp, None).await?;
    sync_for(state, &owner).await
}
pub(super) async fn sync_for(state: &Arc<State>, owner: &Grant) -> Result<Vec<Share>> {
    let _held = owner.mirror.sharing.lock().await;
    sync_held(state, owner).await
}
async fn sync_held(state: &Arc<State>, owner: &Grant) -> Result<Vec<Share>> {
    let dsp = owner.key.dsp.as_str();
    let (drive, mut shares) = on_drive!(
        state,
        owner,
        drive,
        drive
            .google
            .shares(&drive.access, &drive.connection.folder_id)
    )?;
    let at = dsp.to_owned();
    let (wanted, people, name) = owner
        .read(state, move |db, _| {
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
    // Where the folder is shared with each member, for the files shared with them too.
    let mut addresses: Vec<String> = Vec::new();
    // Why Google was asked nothing more: the DSP disabled meanwhile.
    let mut stopped = None;
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
            if let Some((_, old)) = person.shared.clone() {
                if let Err(error) = drive.admit(state).await {
                    stopped = Some(error);
                    break;
                }
                drive.google.unshare(&drive.access, &folder, &old).await?;
                shares.retain(|share| share.id != old);
                person.shared = None;
            }
            if let Err(error) = drive.admit(state).await {
                // Only the share at their old address was taken back, if any.
                if before.is_some_and(|before| before != &person) {
                    changed.push(person);
                }
                stopped = Some(error);
                break;
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
        if let (Sharing::Shared, Some((email, _))) = (&person.sharing, &person.shared) {
            addresses.push(email.clone());
        }
        if before != Some(&person) {
            changed.push(person);
        }
    }
    // Those who left, once the folder is taken back from them.
    let mut gone: Vec<Person> = Vec::new();
    let left = people
        .into_iter()
        .filter(|person| !wanted.iter().any(|member| member.user == person.user));
    for person in left {
        if stopped.is_some() {
            break;
        }
        if let Some((_, id)) = &person.shared {
            if let Err(error) = drive.admit(state).await {
                stopped = Some(error);
                break;
            }
            drive.google.unshare(&drive.access, &folder, id).await?;
            shares.retain(|share| &share.id != id);
        }
        gone.push(person);
    }
    let at = dsp.to_owned();
    let origin = state.config.origin.clone();
    let dev = state.config.environment.is_preview();
    // What Google made is kept, the DSP disabled meanwhile or not: the next sync once it's
    // enabled goes on from there.
    owner
        .run_finished(state, move |db, _| {
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
    if let Some(error) = stopped {
        return Err(error);
    }
    share_files(state, dsp, &drive, &addresses).await?;
    access::current(state, owner).await?;
    Ok(shares)
}

/// Shares each file someone else owns that Documents holds a shortcut to with everyone the
/// folder is shared with, as far as the account may share it, and takes back the shares
/// Dispatch made from those who left, and of files no longer in Documents. Shares anyone else
/// made stay, so someone the file's owner shared it with keeps it.
async fn share_files(
    state: &Arc<State>,
    dsp: &str,
    drive: &access::Drive,
    addresses: &[String],
) -> Result<()> {
    let owner = &drive.owner;
    let tree = access::tree_for(state, owner).await?;
    let targets = tree.targets();
    let account = drive.connection.account.email.to_lowercase();
    let wanted: BTreeSet<String> = addresses
        .iter()
        .map(|email| email.to_lowercase())
        .filter(|email| *email != account)
        .collect();
    let at = dsp.to_owned();
    let made = owner
        .read(state, move |db, _| db.documents_shared_files(&at))
        .await?;
    for file in &targets {
        let missing: Vec<&String> = wanted
            .iter()
            .filter(|email| {
                !made
                    .iter()
                    .any(|shared| &shared.file == file && &shared.email == *email)
            })
            .collect();
        if missing.is_empty() {
            continue;
        }
        drive.admit(state).await?;
        // Its owner may not let the account see who has it, or share it on.
        let Ok(shares) = drive.google.shares(&drive.access, file).await else {
            continue;
        };
        for email in missing {
            let has = shares.iter().any(|share| {
                share
                    .email_address
                    .as_deref()
                    .is_some_and(|other| other.eq_ignore_ascii_case(email))
            });
            if has {
                continue;
            }
            drive.admit(state).await?;
            let share = match drive.google.share(&drive.access, file, email).await {
                Ok(Ok(share)) => share,
                Err(error) if error.code == "google_connection_broken" => return Err(error),
                _ => continue,
            };
            // Kept at once, so a share Dispatch made is always one it can take back: one it
            // couldn't keep is taken back now.
            let shared = SharedFile {
                file: file.clone(),
                email: email.clone(),
                share,
            };
            let (at, kept) = (dsp.to_owned(), shared.clone());
            if let Err(error) = owner
                .run_finished(state, move |db, _| {
                    db.save_documents_shared_file(&at, &kept)
                })
                .await
            {
                // Existing compensation is attempted only while the original grant remains
                // current. Never use a replacement grant to undo an old external effect.
                drive.admit(state).await?;
                drive
                    .google
                    .unshare(&drive.access, &shared.file, &shared.share)
                    .await?;
                return Err(error);
            }
        }
    }
    for shared in made {
        if targets.contains(&shared.file) && wanted.contains(&shared.email) {
            continue;
        }
        drive.admit(state).await?;
        match drive
            .google
            .unshare(&drive.access, &shared.file, &shared.share)
            .await
        {
            // Taken back, or the account no longer reaches the file to take it back.
            Ok(()) => {}
            Err(error) if error.code == "documents_item_not_found" => {}
            Err(error) if error.code == "google_connection_broken" => return Err(error),
            // Google may be busy: it's kept, and the next sync tries again.
            Err(_) => continue,
        }
        let at = dsp.to_owned();
        owner
            .run_finished(state, move |db, _| {
                db.remove_documents_shared_file(&at, &shared)
            })
            .await?;
    }
    Ok(())
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
            Sharing::Shared if moving(&person) => SharingState::Pending,
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
pub fn nudge(state: &Arc<State>, dsp: &str, generation: String) {
    let (state, dsp) = (Arc::clone(state), dsp.to_owned());
    tokio::spawn(async move {
        let Ok(owner) = grant::capture(&state, &dsp, Some(generation)).await else {
            return;
        };
        let Ok(_held) = Arc::clone(&owner.mirror.sharing).try_lock_owned() else {
            return;
        };
        if let Err(error) = sync_held(&state, &owner).await {
            observability::event(
                "warn",
                "documents_share_failed",
                json!({"error": error.code}),
            );
        }
    });
}
/// Every callback retains the grant that requested sharing, including while queued.
pub(super) fn share_soon(state: &Arc<State>, owner: &Grant) {
    let (state, owner) = (Arc::clone(state), owner.clone());
    tokio::spawn(async move {
        if let Err(error) = sync_for(&state, &owner).await {
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
/// Connections or else from Documents, at the address `site`.
pub fn start_link(
    db: &Store,
    c: &Context,
    site: &Site,
    from_settings: bool,
) -> Result<GoogleSignIn> {
    let google = Google::of(&db.config)?;
    let link = if from_settings { FROM_SETTINGS } else { LINK };
    let named = google::sign_in_of(&c.dsp.id, site);
    let state = format!("{named}{link}{}", crypto::token()?);
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
    let owner = grant::capture_member(state, c.clone(), access, None).await?;
    let google = Google::of(&state.config)?;
    let email = google
        .linked_email(&state.config.origin, &code, &verifier)
        .await?;
    let c = owner
        .run(state, move |db, _| {
            let c = access.revalidate(db, &c)?;
            db.link_documents_google(&c.dsp.id, c.actor(), &email)?;
            c.audit(db, "documents.linked", &email)?;
            Ok(c)
        })
        .await?;
    sync_for(state, &owner).await?;
    owner.read(state, move |db, _| mine(db, &c)).await
}

#[cfg(test)]
#[path = "../tests/backend/team.rs"]
mod tests;
