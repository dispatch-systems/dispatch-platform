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
/// it signed if `signed`.
async fn written(state: &Arc<State>, c: &Context, tier: ConductTier, photo: bool, signed: bool) {
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
            if signed {
                let revision = if photo { 2 } else { 1 };
                writeups::sign(db, &c, &id, revision, "2026-11-03", TODAY)?;
            }
            Ok(())
        })
        .await
        .unwrap();
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
    let saving = |name: &str| (TARDIES.to_owned(), name.to_owned(), PHOTO.len(), casey.clone());

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
    assert_eq!(support::folders(&dsp), 3);

    // A folder no longer there is made again, with those inside it.
    support::trash(&dsp, "Writeups/Jordan Gray");
    written(&state, &c, ConductTier::Final, true, true).await;
    back_up(&state, &dsp, 4000).await.unwrap();
    assert_eq!(support::files(&dsp).last(), Some(&saving("2026-11-02 Final.jpg")));
    assert_eq!(support::folders(&dsp), 3);
}

#[tokio::test]
async fn a_copy_that_isnt_saved_is_tried_again_later_and_the_backup_says_its_failing() {
    let (_root, state, c) = platform();
    let dsp = c.dsp.id.clone();
    let going = |failing, why: Option<&str>, waiting| ConductBackup {
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

    // A copy refused is failing too, and waits a minute for its next try.
    support::ready_to_keep(&dsp, true);
    assert_eq!(status(&state, &dsp).await.unwrap(), going(false, None, 1));
    support::refuse(&dsp, true);
    back_up(&state, &dsp, 1000).await.unwrap();
    assert_eq!(status(&state, &dsp).await.unwrap(), going(true, None, 1));
    support::refuse(&dsp, false);
    back_up(&state, &dsp, 1000 + 59 * 1000).await.unwrap();
    assert_eq!(support::files(&dsp), []);
    back_up(&state, &dsp, 1000 + 60 * 1000).await.unwrap();
    assert_eq!(support::files(&dsp).len(), 1);
    assert_eq!(status(&state, &dsp).await.unwrap(), going(false, None, 0));
}
