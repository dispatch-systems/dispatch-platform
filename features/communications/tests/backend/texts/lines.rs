//! Each line's texting its own: a number texting Dispatch Phone's phone and another line's has a
//! conversation on each, every one of another line's a number's, however Routes files the number;
//! each line lists, counts, opens and answers only its own, from its own phones, and names and
//! blocks a number on its own alone, logged as its own; a phone paired while its line's texts
//! weren't filed has those it kept filed, and texts a number first, on its own line alone; another
//! line's page, waiting or answering, goes on with Dispatch Phone off and is refused once its own
//! line is; the minute's upkeep files and fails each line's texts only while its own line is on;
//! and what was kept before the lines were is Dispatch Phone's, left as it was.
use super::{
    contacts, filing,
    model::Filed,
    reading, reply,
    storage::{ContactStore, TextStore},
};
use crate::backend::{
    access::Access,
    changes::ChangeStore,
    inbox, live,
    model::ConversationKind,
    phone::{
        model::{Direction, EXPIRY_MS, Outgoing, PHONE_OFFLINE},
        storage::{DeviceStore, Line, Reported, TextStore as _},
    },
    push::storage::{Address, PushStore, Subscription},
    support::{self, filed, migrate, phoned, phones, started, texted, texted_on, today},
};
use crate::{Communications, Conversation};
use dispatch_core::{
    State,
    db::{Store, now},
    server::http::route::Dsp,
    testing,
};
use std::sync::Arc;

/// Another feature's line, as a test pairs a phone to it.
const DESK: Line = Line {
    id: "front_desk",
    name: "Front Desk",
};
/// What another line's page is behind in these tests: a permission that its own line being off
/// doesn't take away, so its own switch alone refuses it.
const MANAGE: Dsp = Dsp("connections.manage");
/// The number that texts both lines.
const JORDAN: &str = "+14255550199";

/// Pairs the DSP's Front Desk phone, beside its own: its ID.
async fn desk(state: &Arc<State>, dsp: &str) -> String {
    let dsp = dsp.to_owned();
    state
        .run(move |db| Ok(support::pair_to(db, &dsp, DESK, "+12065550111")))
        .await
        .unwrap()
}
/// The page's list of the line `line` as the member `me` reads it.
async fn listed(state: &Arc<State>, dsp: &str, line: Option<&'static str>) -> Communications {
    let (shared, dsp) = (state.clone(), dsp.to_owned());
    state
        .read(move |db| inbox::overview(db, &shared, &dsp, line, "me", None))
        .await
        .unwrap()
}
/// What each conversation listed last says.
fn shown(listed: &Communications) -> Vec<&str> {
    (listed.conversations.iter())
        .filter_map(|each| each.last.as_ref().map(|last| last.text.as_str()))
        .collect()
}
/// The conversation `id` of the line `line` as the page reads it, or why it can't.
async fn one(
    state: &Arc<State>,
    dsp: &str,
    line: Option<&'static str>,
    id: &str,
) -> Result<Conversation, String> {
    let (shared, dsp, id) = (state.clone(), dsp.to_owned(), id.to_owned());
    state
        .read(move |db| reading::one(db, &shared, &dsp, line, &id))
        .await
        .map_err(|error| error.code)
}

#[tokio::test]
async fn a_number_texting_both_lines_has_a_conversation_on_each_and_each_line_reads_only_its_own() {
    let (_root, state, dsp) = started();
    let phone = desk(&state, &dsp).await;
    let ours = texted(&state, &dsp, JORDAN, "Is this dispatch?", now()).await;
    let theirs = texted_on(&state, &dsp, &phone, JORDAN, "About my paycheck", now()).await;

    // Each filed at once, on its own line: two conversations with one number.
    let (ours, theirs) = (
        filed(&state, &dsp, &ours).await,
        filed(&state, &dsp, &theirs).await,
    );
    assert_ne!(ours.id, theirs.id);
    assert_eq!((ours.line, ours.kind), (None, ConversationKind::Number));
    assert_eq!(
        (theirs.line.as_deref(), theirs.kind),
        (Some(DESK.id), ConversationKind::Number)
    );

    // Each line lists, counts and shows its own phones alone; another line has no Slack or
    // Today's Routes.
    let dispatch = listed(&state, &dsp, None).await;
    assert_eq!(
        (dispatch.waiting, shown(&dispatch)),
        (1, vec!["Is this dispatch?"])
    );
    let named = |listed: &Communications| -> Vec<String> {
        listed.phones.iter().map(|each| each.name.clone()).collect()
    };
    assert_eq!(named(&dispatch), ["Dispatch"]);
    let front = listed(&state, &dsp, Some(DESK.id)).await;
    assert_eq!(
        (front.waiting, shown(&front)),
        (1, vec!["About my paycheck"])
    );
    assert_eq!(named(&front), ["Front Desk"]);
    assert!(front.workspaces.is_empty() && !front.routes);
    let (shared, at) = (state.clone(), dsp.clone());
    let waiting = state
        .read(move |db| inbox::waiting(db, &shared, &at, Some(DESK.id)))
        .await
        .unwrap();
    assert_eq!(waiting.count, 1);
    assert_eq!(waiting.latest[0].conversation_id, theirs.id);

    // Neither opens, finds or marks done the other's: another line's conversation isn't there.
    assert!(one(&state, &dsp, Some(DESK.id), &theirs.id).await.is_ok());
    let missing = Err("communications_conversation_not_found".to_owned());
    assert_eq!(
        one(&state, &dsp, None, &theirs.id).await.map(|_| ()),
        missing
    );
    assert_eq!(
        one(&state, &dsp, Some(DESK.id), &ours.id).await.map(|_| ()),
        missing
    );
    let (shared, at, id) = (state.clone(), dsp.clone(), theirs.id.clone());
    let done = state
        .run(move |db| {
            let reference = crate::backend::model::Reference::Text(id);
            crate::backend::dispatcher::done(db, &shared, &at, None, &reference, "me", None)
        })
        .await;
    assert_eq!(done.map(|_| ()).map_err(|error| error.code), missing);
    for (line, id) in [(None, &ours.id), (Some(DESK.id), &theirs.id)] {
        let (shared, at) = (state.clone(), dsp.clone());
        let found = state
            .read(move |db| {
                let kind = ConversationKind::Number;
                reading::search(db, &shared, &at, line, kind, "4255550199")
            })
            .await
            .unwrap();
        let ids: Vec<&str> = found
            .conversations
            .iter()
            .map(|each| each.id.as_str())
            .collect();
        assert_eq!(ids, [id.as_str()]);
    }
}

#[tokio::test]
async fn another_lines_texts_are_its_numbers_even_a_drivers_work_phone_and_those_kept_unfiled_are_filed()
 {
    let (_root, state, dsp) = started();
    let phone = desk(&state, &dsp).await;
    let (_, day) = today(&state, &dsp).await;
    let avery = phones(&[("A1", "2065550101")]);
    phoned(&state, &dsp, Some(&day.to_string()), avery, Vec::new()).await;

    // Avery's work phone texts Dispatch Phone: their work conversation.
    let ours = texted(&state, &dsp, "+12065550101", "Running late", now()).await;
    assert!(filed(&state, &dsp, &ours).await.driver.is_some());

    // It texts the Front Desk too, kept by the phone side before it was filed: the next upkeep
    // files it under the number, on its own line, and filing the day again leaves it.
    let (at, from) = (dsp.clone(), phone.clone());
    let theirs = state
        .run(move |db| {
            let text = Reported {
                client_id: "desk-1".into(),
                direction: Direction::In,
                number: "+12065550101".into(),
                body: "Can I swap shifts?".into(),
                at: now(),
                files: Vec::new(),
            };
            Ok(db.phones_report(&at, &from, vec![text])?.remove(0).id)
        })
        .await
        .unwrap();
    filing::unfiled(&state, &dsp).await.unwrap();
    let kept = filed(&state, &dsp, &theirs).await;
    let number = Some("+12065550101".to_owned());
    assert_eq!(
        (kept.line.as_deref(), kept.kind, &kept.driver, &kept.number),
        (Some(DESK.id), ConversationKind::Number, &None, &number)
    );
    filing::refile_day(&state, &dsp, &day.to_string())
        .await
        .unwrap();
    assert_eq!(filed(&state, &dsp, &theirs).await.id, kept.id);
}

#[tokio::test]
async fn an_answer_goes_from_its_conversations_line_and_each_line_answers_only_its_own() {
    let (_root, state, dsp) = started();
    let phone = desk(&state, &dsp).await;
    let ours = texted(&state, &dsp, JORDAN, "Is this dispatch?", now()).await;
    let theirs = texted_on(&state, &dsp, &phone, JORDAN, "About my paycheck", now()).await;
    let (ours, theirs) = (
        filed(&state, &dsp, &ours).await,
        filed(&state, &dsp, &theirs).await,
    );
    let c = support::member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    let send = |line: Option<&'static str>, id: &str, text: &'static str| {
        let (state, c, id) = (state.clone(), c.clone(), id.to_owned());
        let access = Access {
            grant: support::USE,
            line,
        };
        async move { reply::send(&state, &c, access, &id, text, (&[], None)).await }
    };
    let outbox = |of: String| {
        let (at, state) = (dsp.clone(), state.clone());
        async move {
            let waiting = state
                .read(move |db| db.phones_outbox(&at, &of))
                .await
                .unwrap();
            waiting
                .into_iter()
                .map(|(text, _)| text.body)
                .collect::<Vec<_>>()
        }
    };

    // Each line's answer goes from its own phone alone.
    send(Some(DESK.id), &theirs.id, "Payroll will call you")
        .await
        .unwrap();
    send(None, &ours.id, "Yes, how can we help?").await.unwrap();
    assert_eq!(outbox(phone.clone()).await, ["Payroll will call you"]);
    let dispatch: Vec<String> = (support::sent(&state, &dsp).await.into_iter())
        .map(|(_, body, _)| body)
        .collect();
    assert_eq!(dispatch, ["Yes, how can we help?"]);

    // Neither answers in the other's.
    for (line, id) in [(None, &theirs.id), (Some(DESK.id), &ours.id)] {
        let refused = send(line, id, "Wrong line").await.unwrap_err();
        assert_eq!(refused.code, "communications_conversation_not_found");
    }
    assert_eq!(outbox(phone).await, ["Payroll will call you"]);
}

#[tokio::test]
async fn names_and_blocks_are_each_lines_own_and_logged_as_its_own() {
    let (_root, state, dsp) = started();
    let phone = desk(&state, &dsp).await;
    let ours = texted(&state, &dsp, JORDAN, "Is this dispatch?", now()).await;
    let theirs = texted_on(&state, &dsp, &phone, JORDAN, "About my paycheck", now()).await;
    let (ours, theirs) = (
        filed(&state, &dsp, &ours).await,
        filed(&state, &dsp, &theirs).await,
    );

    // A name the Front Desk saves is its own: Dispatch Phone's conversation is still the number's,
    // and lists no such contact.
    let (shared, at, id) = (state.clone(), dsp.clone(), theirs.id.clone());
    let named = state
        .run(move |db| {
            contacts::save_contact(db, &shared, &at, Some(DESK.id), &id, "Jordan Ellis", "me")
        })
        .await
        .unwrap();
    assert_eq!(named.with.name, "Jordan Ellis");
    let dispatch = one(&state, &dsp, None, &ours.id).await.unwrap();
    assert_eq!(
        (dispatch.with.name.as_str(), dispatch.contact),
        ("(425) 555-0199", false)
    );
    for (line, expected) in [(None, None), (Some(DESK.id), Some("Jordan Ellis"))] {
        let (shared, at) = (state.clone(), dsp.clone());
        let listed = state
            .read(move |db| contacts::contacts(db, &shared, &at, line, None))
            .await
            .unwrap();
        let name = (listed.contacts.iter())
            .find(|each| each.number == JORDAN)
            .map(|each| each.person.name.clone());
        assert_eq!(name.as_deref(), expected, "{line:?}");
    }

    // A block there is its own too: its conversation stops waiting there alone.
    let (shared, at, id) = (state.clone(), dsp.clone(), theirs.id.clone());
    state
        .run(move |db| contacts::block(db, &shared, &at, Some(DESK.id), &id, "me", true))
        .await
        .unwrap();
    assert_eq!(listed(&state, &dsp, Some(DESK.id)).await.waiting, 0);
    let dispatch = listed(&state, &dsp, None).await;
    assert_eq!(dispatch.waiting, 1);
    assert!(!one(&state, &dsp, None, &ours.id).await.unwrap().blocked);

    // Each logged under the line's own actions, never Dispatch Phone's.
    let at = dsp.clone();
    let logged = state
        .read(move |db| {
            let mut logged = testing::audit_actions(db, &at, "communications.");
            logged.extend(testing::audit_actions(db, &at, "front_desk."));
            Ok(logged)
        })
        .await
        .unwrap();
    assert_eq!(
        logged,
        ["front_desk.contact_saved", "front_desk.number_blocked"]
    );
}

#[cfg_attr(target_os = "macos", ignore = "paused clock races I/O; CI runs it")]
#[tokio::test(start_paused = true)]
async fn another_lines_page_goes_on_with_dispatch_phone_off_and_is_refused_once_its_own_is() {
    let (_root, state, dsp) = started();
    let phone = desk(&state, &dsp).await;
    let theirs = texted_on(&state, &dsp, &phone, JORDAN, "About my paycheck", now()).await;
    let theirs = filed(&state, &dsp, &theirs).await;
    let c = support::member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    let access = Access {
        grant: MANAGE,
        line: Some(DESK.id),
    };
    // Who is waiting, let in as each request of theirs would be, as things stand then.
    let context = {
        let (auth, at) = (c.auth.clone(), dsp.clone());
        move |db: &Store| db.context(&auth, &at, MANAGE.0)
    };
    // Its page as it waits on its changes from where they stand as it starts: the `Notify` handed
    // back hears each time this wait alone lets it in.
    let wait = || {
        let (state, dsp, me, context) = (
            state.clone(),
            dsp.clone(),
            c.auth.user.id.clone(),
            context.clone(),
        );
        let read = Arc::new(tokio::sync::Notify::new());
        let again = {
            let read = read.clone();
            move |db: &Store| {
                let c = context(db)?;
                read.notify_one();
                Ok(c)
            }
        };
        let waiting = tokio::spawn(async move {
            let (shared, at) = (state.clone(), dsp.clone());
            let (now_at, _) = state
                .read(move |db| inbox::updates(db, &shared, &at, DESK.id.into(), -1, &me))
                .await?;
            let (after, seen) = (now_at.revision, now_at.typing.version as i64);
            inbox::waited(&state, access, &dsp, again, after, seen).await
        });
        (waiting, read)
    };

    // Its page waits on its changes, with nothing new; Dispatch Phone is switched off and its line
    // changes. It goes on waiting, let in each time, and is answered as its wait ends.
    let (waiting, read) = wait();
    read.notified().await;
    support::switch(&state, &dsp, crate::FEATURE.switch.id, false).await;
    live::changed_on(&state, &dsp, Some(DESK.id));
    let answered = waiting.await.unwrap().map(drop).map_err(|error| error.code);
    assert_eq!(answered, Ok(()));
    // And an answer written now goes from its phone.
    let now_c = state.read(context.clone()).await.unwrap();
    let sent = reply::send(
        &state,
        &now_c,
        access,
        &theirs.id,
        "Payroll will call you",
        (&[], None),
    );
    assert!(sent.await.is_ok());
    let (at, from) = (dsp.clone(), phone.clone());
    let outbox = state.read(move |db| db.phones_outbox(&at, &from)).await;
    assert_eq!(outbox.unwrap().len(), 1);

    // Its page waits again; its own line is switched off and it changes. Read again, it's
    // refused, not answered.
    let (waiting, read) = wait();
    read.notified().await;
    support::switch(&state, &dsp, DESK.id, false).await;
    live::changed_on(&state, &dsp, Some(DESK.id));
    let refused = waiting.await.unwrap().map(drop).map_err(|error| error.code);
    assert_eq!(refused, Err("not_found".to_owned()));

    // Nor does an answer written now go, its access read again as it's queued.
    let now_c = state.read(context).await.unwrap();
    let refused = reply::send(
        &state,
        &now_c,
        access,
        &theirs.id,
        "Payroll will call you again",
        (&[], None),
    )
    .await
    .map(drop)
    .map_err(|error| error.code);
    assert_eq!(refused, Err("not_found".to_owned()));
    let (at, from) = (dsp.clone(), phone.clone());
    let outbox = state.read(move |db| db.phones_outbox(&at, &from)).await;
    assert_eq!(outbox.unwrap().len(), 1);
}

/// Has the phone `from` report Jordan's text `body`, kept unfiled as one that couldn't be placed as
/// it came: its ID.
async fn unplaced(state: &Arc<State>, dsp: &str, from: &str, body: &'static str) -> String {
    let (at, from) = (dsp.to_owned(), from.to_owned());
    state
        .run(move |db| {
            let text = Reported {
                client_id: body.into(),
                direction: Direction::In,
                number: JORDAN.into(),
                body: body.into(),
                at: now(),
                files: Vec::new(),
            };
            Ok(db.phones_report(&at, &from, vec![text])?.remove(0).id)
        })
        .await
        .unwrap()
}
/// An answer to Jordan queued on the line `line`, waiting on its phone: its ID.
async fn queued(state: &Arc<State>, dsp: &str, line: Option<&str>) -> String {
    let (at, line) = (dsp.to_owned(), line.map(str::to_owned));
    state
        .run(move |db| {
            let answer = Outgoing {
                to: JORDAN.into(),
                body: "Payroll will call you".into(),
                files: Vec::new(),
                gif: None,
                by: testing::a_user(db),
                line,
            };
            Ok(db.phones_send(&at, &answer, now())?.id)
        })
        .await
        .unwrap()
}
/// Of the texts `ids`, whether each is filed, and why each failed, if it did.
async fn kept_as(state: &Arc<State>, dsp: &str, ids: &[&str]) -> Vec<(bool, Option<String>)> {
    let (at, ids) = (
        dsp.to_owned(),
        ids.iter().map(|id| id.to_string()).collect::<Vec<_>>(),
    );
    state
        .read(move |db| {
            ids.iter()
                .map(|id| {
                    let filed = db.communications_text(&at, id)?.unwrap().conversation;
                    let error = db.phones_text(&at, id)?.unwrap().error;
                    Ok((filed.is_some(), error))
                })
                .collect()
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn the_minutes_upkeep_files_and_fails_each_lines_texts_only_while_its_own_line_is_on() {
    let (_root, state, dsp) = started();
    let desk = desk(&state, &dsp).await;
    let at = dsp.clone();
    let ours = state
        .read(move |db| Ok(db.phones_devices(&at, None)?.remove(0).id))
        .await
        .unwrap();
    let upkeep = || (filing::MAINTENANCE.run)(state.clone(), true);
    // Past each answer's expiry, its phone offline by then.
    let later = now() + 2 * EXPIRY_MS;
    let failed = Some(PHONE_OFFLINE.to_owned());

    // With Dispatch Phone off, its own line's text and answer wait, and the Front Desk's are filed,
    // its answer as it fails.
    let (our_answer, their_answer) = (
        queued(&state, &dsp, None).await,
        queued(&state, &dsp, Some(DESK.id)).await,
    );
    support::switch(&state, &dsp, crate::FEATURE.switch.id, false).await;
    let (our_text, their_text) = (
        unplaced(&state, &dsp, &ours, "Is this dispatch?").await,
        unplaced(&state, &dsp, &desk, "About my paycheck").await,
    );
    upkeep().await;
    filing::expire(&state, &dsp, later).await.unwrap();
    assert_eq!(
        kept_as(
            &state,
            &dsp,
            &[&our_text, &our_answer, &their_text, &their_answer]
        )
        .await,
        [
            (false, None),
            (false, None),
            (true, None),
            (true, failed.clone())
        ]
    );

    // With Dispatch Phone on and the Front Desk off, the other way around.
    support::switch(&state, &dsp, crate::FEATURE.switch.id, true).await;
    support::switch(&state, &dsp, DESK.id, false).await;
    let (their_next, their_last) = (
        unplaced(&state, &dsp, &desk, "It's short this week").await,
        queued(&state, &dsp, Some(DESK.id)).await,
    );
    upkeep().await;
    filing::expire(&state, &dsp, later).await.unwrap();
    assert_eq!(
        kept_as(
            &state,
            &dsp,
            &[&our_text, &our_answer, &their_next, &their_last]
        )
        .await,
        [(true, None), (true, failed), (false, None), (false, None)]
    );
}

#[tokio::test]
async fn a_phone_paired_while_its_lines_texts_werent_filed_has_them_filed_and_texts_first_on_its_line()
 {
    let (_root, state, dsp) = started();
    let phone = desk(&state, &dsp).await;
    let ours = texted(&state, &dsp, JORDAN, "Is this dispatch?", now()).await;
    let ours = filed(&state, &dsp, &ours).await;

    // Paired as the release that kept another line's texts unfiled paired it, which kept one: the
    // next upkeep files it on its own line, and what it relays from then on is filed as it comes.
    let (at, from) = (dsp.clone(), phone.clone());
    let kept = state
        .run(move |db| {
            (db.dsp(&at)?).exec("UPDATE phones_devices SET texting=0 WHERE id=?", [&from])?;
            let text = Reported {
                client_id: "night-1".into(),
                direction: Direction::In,
                number: JORDAN.into(),
                body: "About my paycheck".into(),
                at: now(),
                files: Vec::new(),
            };
            Ok(db.phones_report(&at, &from, vec![text])?.remove(0).id)
        })
        .await
        .unwrap();
    filing::unfiled(&state, &dsp).await.unwrap();
    let theirs = filed(&state, &dsp, &kept).await;
    assert_eq!(
        (theirs.line.as_deref(), theirs.kind, &theirs.number),
        (
            Some(DESK.id),
            ConversationKind::Number,
            &Some(JORDAN.to_owned())
        )
    );
    let more = texted_on(&state, &dsp, &phone, JORDAN, "It's short this week", now()).await;
    assert_eq!(filed(&state, &dsp, &more).await.id, theirs.id);

    // Its line lists them; Dispatch Phone's lists its own alone.
    let front = listed(&state, &dsp, Some(DESK.id)).await;
    assert_eq!(
        (front.waiting, shown(&front)),
        (1, vec!["It's short this week"])
    );
    let dispatch = listed(&state, &dsp, None).await;
    assert_eq!(
        (dispatch.waiting, shown(&dispatch)),
        (1, vec!["Is this dispatch?"])
    );

    // Texting the number first opens its conversation on each line, each its own line's alone.
    let open = |line: Option<Line<'static>>| {
        let (shared, at) = (state.clone(), dsp.clone());
        state.run(move |db| contacts::open(db, &shared, &at, line, JORDAN))
    };
    let (front, dispatch) = (open(Some(DESK)).await.unwrap(), open(None).await.unwrap());
    assert_eq!(
        (front.id, dispatch.id),
        (theirs.id.clone(), ours.id.clone())
    );
    let missing = Err("communications_conversation_not_found".to_owned());
    assert_eq!(
        one(&state, &dsp, None, &theirs.id).await.map(|_| ()),
        missing
    );
}

#[tokio::test]
async fn what_was_kept_before_the_lines_is_dispatch_phones_own_and_left_as_it_was() {
    let (_root, state, dsp) = started();
    let at = dsp.clone();
    let (old, revision) = state
        .run(move |db| {
            // The tables as migration 23 left them, with a conversation, a name, a block, a browser
            // and a change kept.
            let before = db.dsp(&at)?;
            before.0.execute_batch(
                "DROP INDEX communications_text_conversations_lines; \
                 ALTER TABLE communications_text_conversations DROP COLUMN line; \
                 CREATE UNIQUE INDEX communications_text_conversations_numbers \
                 ON communications_text_conversations (kind, number) WHERE driver_id IS NULL; \
                 DROP INDEX communications_changes_lines; \
                 ALTER TABLE communications_changes DROP COLUMN line; \
                 DROP TABLE communications_line_contacts; \
                 DROP TABLE communications_line_blocked; \
                 DROP TABLE communications_line_push; \
                 ALTER TABLE phones_devices DROP COLUMN texting; \
                 ALTER TABLE phones_pairings DROP COLUMN line_texting; \
                 INSERT INTO communications_text_conversations (id,kind,number,state,last_at) \
                 VALUES ('txt_kept','number','+14255550199','responded',1000); \
                 INSERT INTO communications_contacts VALUES ('+14255550199','Jordan','me',1000); \
                 INSERT INTO communications_blocked VALUES ('+18885550142','me',1000); \
                 INSERT INTO communications_push VALUES \
                 ('https://fcm.googleapis.com/fcm/send/kept','me','key','secret','admin',1000); \
                 INSERT INTO communications_changes (conversation_id) VALUES ('txt_kept');",
            )?;
            let revision = before.count("SELECT max(revision) FROM communications_changes", [])?;
            // Dispatch Phone's names, blocks and browsers, as the release before the lines made
            // them: their tables, their indexes and their rows.
            let kept = |db: &dispatch_core::db::Db| -> dispatch_core::Result<Vec<Vec<String>>> {
                let mut kept: Vec<Vec<String>> = db
                    .query_as(
                        "SELECT ifnull(sql,'') FROM sqlite_master WHERE tbl_name IN \
                     ('communications_contacts','communications_blocked','communications_push') \
                     ORDER BY name",
                        [],
                    )?
                    .into_iter()
                    .map(|(sql,): (String,)| vec![sql])
                    .collect();
                kept.extend(
                    db.query_as::<(String, String)>(
                        "SELECT number,name FROM communications_contacts UNION ALL \
                     SELECT number,blocked_by FROM communications_blocked UNION ALL \
                     SELECT endpoint,user_id FROM communications_push",
                        [],
                    )?
                    .into_iter()
                    .map(|(key, by)| vec![key, by]),
                );
                Ok(kept)
            };
            let was = kept(&before)?;
            drop(before);
            // Taken twice, as a restart cut short may: the second changes nothing.
            migrate(db, &at, &[24, 24]);
            let after = db.dsp(&at)?;
            assert_eq!(kept(&after)?, was);
            // The release before the lines still saves a name and keeps a browser as it did.
            after.0.execute_batch(
                "INSERT INTO communications_contacts (number,name,saved_by,saved_at) \
                 VALUES ('+14255550199','Jordan','me',2000) ON CONFLICT(number) DO UPDATE \
                 SET name=excluded.name,saved_by=excluded.saved_by,saved_at=excluded.saved_at; \
                 INSERT INTO communications_push (endpoint,user_id,p256dh,auth,site,created_at) \
                 VALUES ('https://fcm.googleapis.com/fcm/send/kept','me','key','secret','admin',\
                 1000) ON CONFLICT (endpoint) DO UPDATE SET user_id=excluded.user_id;",
            )?;
            drop(after);
            Ok((
                db.communications_text_conversation(&at, "txt_kept")?,
                revision,
            ))
        })
        .await
        .unwrap();

    // Each is Dispatch Phone's, as it was, and none another line's.
    assert_eq!(old.map(|kept| kept.line), Some(None));
    let at = dsp.clone();
    let (kept, other) = state
        .read(move |db| {
            let of = |line| -> dispatch_core::Result<_> {
                let pushes = db.communications_pushes(&at, line)?;
                Ok((
                    db.communications_contacts(&at, line)?,
                    db.communications_blocked(&at, line)?
                        .into_iter()
                        .collect::<Vec<_>>(),
                    pushes
                        .into_iter()
                        .map(|each| each.endpoint)
                        .collect::<Vec<_>>(),
                    db.communications_revision(&at, line)?,
                ))
            };
            Ok((of(None)?, of(Some(DESK.id))?))
        })
        .await
        .unwrap();
    let endpoint = "https://fcm.googleapis.com/fcm/send/kept".to_owned();
    assert_eq!(
        kept,
        (
            vec![(JORDAN.to_owned(), "Jordan".to_owned())],
            vec!["+18885550142".to_owned()],
            vec![endpoint.clone()],
            revision
        )
    );
    assert_eq!(other, (Vec::new(), Vec::new(), Vec::new(), 0));
    assert!(revision > 0);

    // Another line keeps its own beside them, with the same number and the same browser.
    let at = dsp.clone();
    let theirs = state
        .run(move |db| {
            let browser = Subscription {
                endpoint,
                user: "me".into(),
                p256dh: "key".into(),
                auth: "secret".into(),
                address: Address::Admin,
            };
            db.keep_communications_push(&at, Some(DESK.id), &browser)?;
            db.save_communications_contact(&at, Some(DESK.id), JORDAN, "Jordan Ellis", "me")?;
            let filed = Filed::Number(JORDAN.to_owned());
            let theirs = db.open_communications_text(&at, Some(DESK.id), &filed)?;
            Ok((theirs.id, db.communications_contacts(&at, None)?))
        })
        .await
        .unwrap();
    assert_ne!(theirs.0, "txt_kept");
    assert_eq!(theirs.1, [(JORDAN.to_owned(), "Jordan".to_owned())]);
}
