//! Backing signed copies up, against a stand-in for where the DSP keeps its files.
use super::*;
use crate::{
    api::types::WriteupInput,
    backend::{
        settings::BACKUP,
        support::{self, platform},
        writeups::{self, Subject},
    },
};
use dispatch_core::{accounts::Context, settings as kept_settings};
use dispatch_documents_contract::Author;

const TODAY: &str = "2026-11-08";
/// A JPEG's first bytes: all a photo needs to be kept as one.
const PHOTO: [u8; 4] = [0xff, 0xd8, 0xff, 0xdb];
/// Where Jordan Gray's tardies go.
const TARDIES: &str = "Writeups/Jordan Gray/Excessive Tardies";

/// Writes Jordan Gray up for tardies at `tier`, with a photo of the paper if `photo`, and marks
/// it signed if `signed`: its ID and revision.
async fn written(
    state: &Arc<State>,
    c: &Context,
    tier: ConductTier,
    photo: bool,
    signed: bool,
) -> (String, i64) {
    let c = c.clone();
    state
        .run(move |db| {
            let subject = Subject {
                code: "P-0001".into(),
                name: "Jordan Gray".into(),
            };
            let writing = WriteupInput {
                person: "P-0001".into(),
                category: "tardies".into(),
                tier,
                final_kind: None,
                suspension: None,
                written_on: "2026-11-02".into(),
                statement: "DA had excessive tardies on 10/30, 11/1 and 11/2.".into(),
                prior: "DA is aware of the start time.".into(),
                plan: "DA will arrive on time.".into(),
                revision: None,
            };
            let id = writeups::add(db, &c, &subject, &writing, TODAY)?;
            if photo {
                copies::keep(db, &c, &id, 1, &PHOTO)?;
            }
            let mut revision = if photo { 2 } else { 1 };
            if signed {
                writeups::sign(db, &c, &id, revision, "2026-11-03", TODAY)?;
                revision += 1;
            }
            Ok((id, revision))
        })
        .await
        .unwrap()
}
/// Deletes the writeup `id` as read at `revision`, as its route does: its saved copy taken
/// back first.
async fn deleted(state: &Arc<State>, c: &Context, id: &str, revision: i64) -> Result<()> {
    let (by, of) = (c.clone(), id.to_owned());
    let copy = state.read(move |db| saved(db, &by, &of, revision)).await?;
    if let Some((files, saved)) = copy {
        take_back(state, c, files, &saved).await?;
    }
    let (by, of) = (c.clone(), id.to_owned());
    state
        .run(move |db| writeups::delete(db, &by, &of, revision))
        .await
}
/// Has the DSP back its signed copies up, as its Settings saves it.
async fn choose(state: &Arc<State>, c: &Context) {
    let c = c.clone();
    state
        .run(move |db| {
            let on = json!({"signed_copies": true});
            kept_settings::save(db, &c.dsp.id, c.actor(), &BACKUP, &on, 0, false).map(drop)
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn signed_copies_go_in_their_persons_and_categorys_folder_once_the_dsp_backs_them_up() {
    let (_root, state, c) = platform();
    let dsp = c.dsp.id.clone();
    let casey = Author::Named("Casey Supervisor".into());
    let saving = |name: &str| {
        (
            TARDIES.to_owned(),
            name.to_owned(),
            PHOTO.len(),
            casey.clone(),
        )
    };

    // Until the DSP chooses to, nothing is saved.
    written(&state, &c, ConductTier::Verbal, true, true).await;
    back_up(&state, &dsp, 1000).await.unwrap();
    assert_eq!(support::files(&dsp), []);

    // Once it does, the copy signed before goes too, named for its date and tier, from who
    // marked it signed; a writeup signed without a photo, or not yet signed, has none to save.
    written(&state, &c, ConductTier::Written, false, true).await;
    written(&state, &c, ConductTier::Written, true, false).await;
    choose(&state, &c).await;
    back_up(&state, &dsp, 1000).await.unwrap();
    assert_eq!(support::files(&dsp), [saving("2026-11-02 Verbal.jpg")]);

    // Saved once, and the next goes in the folders the first made.
    written(&state, &c, ConductTier::Written, true, true).await;
    back_up(&state, &dsp, 2000).await.unwrap();
    back_up(&state, &dsp, 3000).await.unwrap();
    let both = [
        saving("2026-11-02 Verbal.jpg"),
        saving("2026-11-02 Written.jpg"),
    ];
    assert_eq!(support::files(&dsp), both);
    // The activity log is given no driver's name for their folder.
    let folders = ["Writeups", "a driver's folder", "Excessive Tardies"];
    assert_eq!(support::folders(&dsp), folders);

    // A folder no longer there is made again, with the one inside it, for the next copy.
    support::trash(&dsp, "Writeups/Jordan Gray");
    written(&state, &c, ConductTier::Final, true, true).await;
    back_up(&state, &dsp, 4000).await.unwrap();
    assert_eq!(support::files(&dsp), [saving("2026-11-02 Final.jpg")]);
    assert_eq!(support::folders(&dsp), folders);
}

#[tokio::test]
async fn a_copy_that_isnt_saved_is_tried_again_later_and_the_backup_says_its_failing() {
    let (_root, state, c) = platform();
    let dsp = c.dsp.id.clone();
    let going = |failing, why: Option<&str>, waiting| ConductBackup {
        place: Some("Kept Drive".into()),
        failing,
        why: why.map(str::to_owned),
        waiting,
    };

    // A backup the DSP hasn't chosen isn't failing, whatever keeps its files says.
    support::ready_to_keep(&dsp, false);
    written(&state, &c, ConductTier::Verbal, true, true).await;
    assert_eq!(status(&state, &dsp).await.unwrap(), going(false, None, 0));

    // Chosen, it says why its copies can't be kept now, and tries none.
    choose(&state, &c).await;
    back_up(&state, &dsp, 1000).await.unwrap();
    assert_eq!(
        status(&state, &dsp).await.unwrap(),
        going(true, Some("Not ready"), 1)
    );

    // A copy refused is failing too, for the reason what keeps the files gives, and waits a
    // minute for its next try.
    support::ready_to_keep(&dsp, true);
    assert_eq!(status(&state, &dsp).await.unwrap(), going(false, None, 1));
    support::refuse(&dsp, Some(support::FULL));
    back_up(&state, &dsp, 1000).await.unwrap();
    assert_eq!(
        status(&state, &dsp).await.unwrap(),
        going(true, Some("Kept Drive is full"), 1)
    );
    support::refuse(&dsp, None);
    back_up(&state, &dsp, 1000 + 59 * 1000).await.unwrap();
    assert_eq!(support::files(&dsp), []);
    back_up(&state, &dsp, 1000 + 60 * 1000).await.unwrap();
    assert_eq!(support::files(&dsp).len(), 1);
    assert_eq!(status(&state, &dsp).await.unwrap(), going(false, None, 0));
}

#[tokio::test]
async fn once_conduct_is_switched_off_no_more_copies_are_sent_in_the_pass_under_way() {
    let (_root, state, c) = platform();
    let dsp = c.dsp.id.clone();
    written(&state, &c, ConductTier::Verbal, true, true).await;
    written(&state, &c, ConductTier::Written, true, true).await;
    choose(&state, &c).await;

    support::switch_off_on_save(&dsp);
    back_up(&state, &dsp, 1000).await.unwrap();
    assert_eq!(support::files(&dsp).len(), 1);
}

#[tokio::test]
async fn the_backup_says_why_the_copy_tried_latest_wasnt_saved() {
    let (_root, state, c) = platform();
    let dsp = c.dsp.id.clone();
    choose(&state, &c).await;

    // One copy refused twice waits five minutes for its third try.
    written(&state, &c, ConductTier::Verbal, true, true).await;
    support::refuse(&dsp, Some(support::FULL));
    back_up(&state, &dsp, 0).await.unwrap();
    back_up(&state, &dsp, 60 * 1000).await.unwrap();
    // The next, refused once since and for another reason, is due before it, and tried later.
    written(&state, &c, ConductTier::Written, true, true).await;
    support::refuse(&dsp, Some(support::BUSY));
    back_up(&state, &dsp, 61 * 1000).await.unwrap();
    let going = status(&state, &dsp).await.unwrap();
    assert_eq!(going.why.as_deref(), Some("Kept Drive is busy"));
    assert_eq!(going.waiting, 2);
}

#[tokio::test]
async fn a_deleted_writeups_copy_is_taken_back_while_the_dsp_backs_its_copies_up() {
    let (_root, state, c) = platform();
    let dsp = c.dsp.id.clone();
    choose(&state, &c).await;
    let (verbal, at) = written(&state, &c, ConductTier::Verbal, true, true).await;
    let (written_up, _) = written(&state, &c, ConductTier::Written, true, true).await;
    back_up(&state, &dsp, 1000).await.unwrap();
    assert_eq!(support::files(&dsp).len(), 2);

    // While the copy can't be taken back, the writeup stays, and its copy with it.
    support::refuse(&dsp, Some(support::BUSY));
    let refused = deleted(&state, &c, &verbal, at).await;
    assert_eq!(refused.unwrap_err().code, "conduct_copy_not_removed");
    assert_eq!(support::files(&dsp).len(), 2);
    let (at_dsp, id) = (dsp.clone(), verbal.clone());
    state
        .read(move |db| writeups::read(db, &at_dsp, &id))
        .await
        .expect("the writeup stays");

    // Once it can, the copy goes as the member deleting it asked, then the writeup; the
    // other's stays.
    support::refuse(&dsp, None);
    deleted(&state, &c, &verbal, at).await.unwrap();
    let kept: Vec<String> = support::files(&dsp)
        .into_iter()
        .map(|(_, name, ..)| name)
        .collect();
    assert_eq!(kept, ["2026-11-02 Written.jpg"]);
    assert_eq!(
        support::removed(&dsp),
        [Author::Member(c.actor().to_owned())]
    );

    // A copy already gone from the DSP's files doesn't keep its writeup.
    support::trash(&dsp, "Writeups");
    deleted(&state, &c, &written_up, at).await.unwrap();

    // Once the DSP no longer backs its copies up, the copy saved stays where it is.
    let (last, at) = written(&state, &c, ConductTier::Final, true, true).await;
    back_up(&state, &dsp, 2000).await.unwrap();
    assert_eq!(support::files(&dsp).len(), 1);
    let off = c.clone();
    state
        .run(move |db| {
            let chosen = json!({"signed_copies": false});
            kept_settings::save(db, &off.dsp.id, off.actor(), &BACKUP, &chosen, 1, false).map(drop)
        })
        .await
        .unwrap();
    deleted(&state, &c, &last, at).await.unwrap();
    assert_eq!(support::files(&dsp).len(), 1);
}

#[tokio::test]
async fn a_copy_saved_as_its_writeup_is_deleted_goes_with_it() {
    let (_root, state, c) = platform();
    let dsp = c.dsp.id.clone();
    choose(&state, &c).await;
    let (id, at) = written(&state, &c, ConductTier::Verbal, true, true).await;

    let by = c.clone();
    support::on_save(&dsp, move |db, _| writeups::delete(db, &by, &id, at));
    back_up(&state, &dsp, 1000).await.unwrap();
    assert_eq!(support::files(&dsp), []);
}
