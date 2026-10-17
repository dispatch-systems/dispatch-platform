//! Driver Match: every ID the collections hold gets one person's code, certain names join
//! on their own, and the rest wait for a decision that merges, splits or keeps apart.
use dispatch_core::testing as common;
use dispatch_core::{
    accounts::{Auth, Context},
    db::{Store, s},
    manifest::{Collector, people::DriverSource},
};
use dispatch_cortex::{associates, discovery::Scope, meals};
use dispatch_driver_match::{
    Driver, DriverEventKind, DriverLink, DriverMatch, DriverMatchStore, DriverRole, DriverStrength,
    valid_code,
};
use dispatch_driver_profiles::DriverProfilesStore;
use dispatch_paycom::fixtures;
use dispatch_timecard::TimecardStore;
use serde_json::{Value, json};

/// A day inside the fixture's pay period, so its timecards exist whatever today is.
const DAY: chrono::NaiveDate = chrono::NaiveDate::from_ymd_opt(2026, 9, 12).unwrap();

/// A DSP whose Paycom roster and Amazon meal breaks name the same people in different
/// ways: a short form, a two-part surname, an exact match, a driver Paycom lacks and an
/// office employee. `saved` stands for links made on the meal-break page.
fn ready(saved: Option<Value>) -> (tempfile::TempDir, Store, String, Context) {
    let (root, db, id) = common::bootstrapped();
    db.enable_all_features(&id).unwrap();
    if let Some(saved) = saved {
        db.dsp(&id)
            .unwrap()
            .set("employees.provider_links", &saved)
            .unwrap();
    }
    let mut roster = fixtures::fixture_date("UTC", Some(DAY)).unwrap();
    for (code, name) in [
        ("E002", "REYES, ANTONIO"),
        ("E003", "HERNANDEZ ORTIZ, LUIS"),
        ("E005", "OKAFOR, SHANICE"),
    ] {
        let employee = roster["employees"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|e| e["code"] == code)
            .unwrap();
        employee["name"] = json!(name);
    }
    db.publish_timecards(&id, &roster).unwrap();
    let scope = Scope {
        date: s(&roster, "from").to_owned(),
        station: "DEMO1".into(),
        service_area_id: "area-demo".into(),
        provider: "provider-demo".into(),
        timezone: "UTC".into(),
    };
    let mut capture = meals::fixture(&scope);
    let base = capture.itineraries.remove(0);
    for (transporter, name) in [
        ("TREYES", "Tony Reyes"),
        ("TLUIS", "Luis Hernandez"),
        ("TTAYLOR", "Taylor Brooks"),
        ("TSHAY", "Shay Okafor"),
        ("TDMITRI", "Dmitri Volkov"),
    ] {
        let mut itinerary = base.clone();
        itinerary.id = format!("itinerary-{transporter}");
        itinerary.transporter_id = transporter.into();
        itinerary.driver = name.into();
        capture.itineraries.push(itinerary);
    }
    db.publish_meals(&id, "job-meals", &capture, &scope)
        .unwrap();
    let context = context(&db, &id);
    (root, db, id, context)
}

fn context(db: &Store, id: &str) -> Context {
    let owner = db
        .platform
        .one("SELECT id FROM users LIMIT 1", [])
        .unwrap()
        .unwrap();
    let auth = Auth {
        user: serde_json::from_value(
            json!({"id":owner["id"],"email":"","firstName":"","lastName":"","platformOwner":true}),
        )
        .unwrap(),
        hash: String::new(),
        csrf: String::new(),
        raw: String::new(),
        preview: None,
        site: dispatch_core::foundation::config::Site::Admin,
        scope: None,
    };
    db.context(&auth, id, "driver_match.manage").unwrap()
}

/// The code holding an ID, in a Driver Match answer.
fn code(result: &DriverMatch, source: DriverSource, id: &str) -> String {
    result
        .drivers
        .iter()
        .find(|d| d.ids.iter().any(|i| i.source == source && i.id == id))
        .map(|d| d.code.clone())
        .unwrap_or_else(|| panic!("{id} has no person"))
}
fn person<'a>(result: &'a DriverMatch, code: &str) -> &'a Driver {
    result.drivers.iter().find(|d| d.code == code).unwrap()
}
/// The sources whose IDs a person holds, in their order.
fn known_to(result: &DriverMatch, code: &str) -> Vec<&'static str> {
    person(result, code)
        .ids
        .iter()
        .map(|i| i.source.as_str())
        .collect()
}
/// The subject the platform's activity log shows for the newest event of `action`.
fn shown(db: &Store, action: &str) -> String {
    db.audit_page(&dispatch_core::db::AuditQuery::default())
        .unwrap()
        .events
        .into_iter()
        .find(|e| e.action == action)
        .and_then(|e| e.target)
        .unwrap_or_else(|| panic!("no {action}"))
}
const PAYCOM: DriverSource = DriverSource::new(&dispatch_paycom::PEOPLE);
const AMAZON: DriverSource = DriverSource::new(&dispatch_cortex::PEOPLE);

#[test]
fn every_id_gets_one_code_and_only_certain_names_join_on_their_own() {
    dispatch_backend::install();
    let (_root, db, id, context) = ready(None);
    // Twelve employees and five drivers.
    assert_eq!(db.match_drivers(&id).unwrap(), 17);
    let result = db.driver_match(&id).unwrap();
    assert_eq!(result.drivers.len(), 15);
    assert!(result.checked_at.is_some());
    assert!(result.drivers.iter().all(|d| valid_code(&d.code)));
    // Every code is listed once on the platform.
    assert_eq!(
        db.platform
            .count("SELECT count(*) FROM driver_codes WHERE dsp_id=?", [&id])
            .unwrap(),
        15
    );
    assert_eq!(result.sources, [PAYCOM, AMAZON]);
    let taylor = code(&result, PAYCOM, "E004");
    assert_eq!(taylor, code(&result, AMAZON, "TTAYLOR"));
    assert_eq!(person(&result, &taylor).link, DriverLink::Name);
    let luis = code(&result, PAYCOM, "E003");
    assert_eq!(luis, code(&result, AMAZON, "TLUIS"));
    assert_eq!(person(&result, &luis).link, DriverLink::Variant);
    assert_eq!(result.counts.unconfirmed, 1);
    // A short form is suggested, never joined by itself.
    let antonio = code(&result, PAYCOM, "E002");
    let tony = code(&result, AMAZON, "TREYES");
    assert_ne!(antonio, tony);
    assert!(person(&result, &antonio).review);
    // Nobody is office staff until the DSP says which departments drive.
    let avery = code(&result, PAYCOM, "E001");
    assert_eq!(known_to(&result, &avery), ["paycom"]);
    assert_eq!(person(&result, &avery).role, DriverRole::Driver);
    let dmitri = code(&result, AMAZON, "TDMITRI");
    assert_eq!(known_to(&result, &dmitri), ["amazon"]);
    assert_eq!(result.counts.review, 2);
    assert_eq!(result.counts.office, 0);
    // Amazon's drivers merge into Paycom's employees, whose codes they keep.
    let reyes = result
        .review
        .iter()
        .find(|pair| pair.candidate.code == antonio)
        .unwrap();
    assert_eq!(reyes.person.code, tony);
    assert_eq!(reyes.strength, DriverStrength::Strong);
    // The Timecard settings' driver departments set the rest apart as office staff.
    let mut values = db.timecard_preference_values(&id).unwrap()["values"].clone();
    values["driver_departments"] = json!(["Delivery"]);
    db.save_timecard_preferences(&id, &context.auth.user.id, 0, &values)
        .unwrap();
    let staffed = db.driver_match(&id).unwrap();
    assert_eq!(person(&staffed, &avery).role, DriverRole::Office);
    assert_eq!(staffed.counts.office, 1);
    assert_eq!(staffed.counts.drivers, staffed.counts.people - 1);
    // Nothing new the second time, and nobody's code changes.
    assert_eq!(db.match_drivers(&id).unwrap(), 0);
    let again = db.driver_match(&id).unwrap();
    assert_eq!(code(&again, PAYCOM, "E002"), antonio);
    assert_eq!(code(&again, AMAZON, "TLUIS"), luis);
}

#[test]
fn a_merge_makes_one_person_and_the_old_code_still_finds_them() {
    dispatch_backend::install();
    let (_root, db, id, context) = ready(None);
    db.match_drivers(&id).unwrap();
    let before = db.driver_match(&id).unwrap();
    let (antonio, tony) = (
        code(&before, PAYCOM, "E002"),
        code(&before, AMAZON, "TREYES"),
    );
    let after = db.merge_drivers(&context, &tony, &antonio).unwrap();
    assert_eq!(code(&after, AMAZON, "TREYES"), antonio);
    assert_eq!(person(&after, &antonio).link, DriverLink::Person);
    assert_eq!(after.counts.review, 1);
    assert_eq!(after.drivers.len(), before.drivers.len() - 1);
    let details = db.driver_details(&id, &tony).unwrap();
    assert_eq!(details.driver.code, antonio);
    assert_eq!(details.merged_codes, std::slice::from_ref(&tony));
    let merged = details
        .history
        .iter()
        .find(|e| e.kind == DriverEventKind::Merged)
        .unwrap();
    assert_eq!(merged.code.as_deref(), Some(tony.as_str()));
    assert_eq!(merged.actor.as_deref(), Some("Platform support"));
    // Fourteen days, as Paycom and Amazon each saw them.
    assert_eq!(details.dates.len(), 14);
    assert_eq!(
        details
            .days
            .iter()
            .map(|d| (d.source, d.worked.len()))
            .collect::<Vec<_>>(),
        [(PAYCOM, 14), (AMAZON, 14)]
    );
    // Merging what is already merged is refused, as the page is out of date.
    let stale = db.merge_drivers(&context, &tony, &antonio).unwrap_err();
    assert_eq!(stale.code, "driver_changed");
    let merge = db
        .platform
        .one("SELECT * FROM audit WHERE action='driver_match.merged'", [])
        .unwrap()
        .unwrap();
    assert_eq!(s(&merge, "dsp_id"), id);
    assert_eq!(s(&merge, "detail"), format!("{tony} into {antonio}"));
    // The entry names codes: what a collection says about someone stays in its database.
    let data: serde_json::Value = serde_json::from_str(s(&merge, "data")).unwrap();
    assert_eq!(data["target"], format!("{tony} and {antonio}"));
    // The log reads who they are from the collections as it is shown.
    assert_eq!(shown(&db, "driver_match.merged"), "Antonio Reyes");
}

/// A person confirms what a match by a name or a contact only suggested; a link someone
/// already decided, or an ID's first, has nothing to confirm.
#[test]
fn only_a_match_by_a_name_or_a_contact_is_confirmed() {
    dispatch_backend::install();
    let (_root, db, id, context) = ready(None);
    db.match_drivers(&id).unwrap();
    let luis = code(&db.driver_match(&id).unwrap(), PAYCOM, "E003");
    let confirm = |source, external: &str| db.confirm_driver(&context, &luis, source, external);
    assert_eq!(confirm(PAYCOM, "E003").unwrap_err().code, "driver_changed");
    let after = confirm(AMAZON, "TLUIS").unwrap();
    let link = |result: &DriverMatch| {
        person(result, &luis)
            .ids
            .iter()
            .find(|i| i.id == "TLUIS")
            .unwrap()
            .linked_by
    };
    assert_eq!(link(&after), DriverLink::Person);
    assert_eq!(confirm(AMAZON, "TLUIS").unwrap_err().code, "driver_changed");
}

#[test]
fn people_split_or_kept_apart_are_not_suggested_again() {
    dispatch_backend::install();
    let (_root, db, id, context) = ready(None);
    db.match_drivers(&id).unwrap();
    let before = db.driver_match(&id).unwrap();
    let (antonio, tony) = (
        code(&before, PAYCOM, "E002"),
        code(&before, AMAZON, "TREYES"),
    );
    db.merge_drivers(&context, &tony, &antonio).unwrap();
    let split = db
        .split_driver(&context, &antonio, AMAZON, "TREYES")
        .unwrap();
    let fresh = code(&split, AMAZON, "TREYES");
    assert!(fresh != antonio && fresh != tony && valid_code(&fresh));
    assert!(
        split
            .review
            .iter()
            .all(|pair| pair.person.code != antonio && pair.candidate.code != antonio)
    );
    let refused = db
        .split_driver(&context, &fresh, AMAZON, "TREYES")
        .unwrap_err();
    assert_eq!(refused.code, "driver_single_id");
    let (shanice, shay) = (code(&split, PAYCOM, "E005"), code(&split, AMAZON, "TSHAY"));
    let apart = db.keep_drivers_apart(&context, &shay, &shanice).unwrap();
    assert!(apart.review.is_empty());
    assert_eq!(known_to(&apart, &shanice), ["paycom"]);
    assert!(!person(&apart, &shanice).review);
    let history = db.driver_details(&id, &shay).unwrap().history;
    assert!(
        history.iter().any(
            |e| e.kind == DriverEventKind::Apart && e.code.as_deref() == Some(shanice.as_str())
        )
    );
    assert_eq!(shown(&db, "driver_match.split"), "Tony Reyes");
    assert_eq!(
        shown(&db, "driver_match.kept_apart"),
        "Shay Okafor and Shanice Okafor"
    );
}

#[test]
fn someone_only_amazon_saw_long_ago_has_left_and_is_not_asked_about() {
    dispatch_backend::install();
    let (_root, db, id, _) = ready(None);
    let old = Scope {
        date: (DAY - chrono::Duration::days(40)).to_string(),
        station: "DEMO1".into(),
        service_area_id: "area-demo".into(),
        provider: "provider-demo".into(),
        timezone: "UTC".into(),
    };
    let mut capture = meals::fixture(&old);
    capture.itineraries.truncate(1);
    let gone = &mut capture.itineraries[0];
    gone.id = "itinerary-TGONE".into();
    gone.transporter_id = "TGONE".into();
    // Paycom's Jamie Collins shares the last name.
    gone.driver = "Morgan Collins".into();
    db.publish_meals(&id, "job-old-meals", &capture, &old)
        .unwrap();
    db.match_drivers(&id).unwrap();
    let result = db.driver_match(&id).unwrap();
    let gone = code(&result, AMAZON, "TGONE");
    assert_eq!(person(&result, &gone).role, DriverRole::Former);
    // Seen lately, Dmitri still waits for someone to find them in Paycom.
    let dmitri = code(&result, AMAZON, "TDMITRI");
    assert_eq!(person(&result, &dmitri).role, DriverRole::Driver);
    // Nobody asks whether someone who has left is someone else.
    assert!(
        result
            .review
            .iter()
            .all(|pair| pair.person.code != gone && pair.candidate.code != gone)
    );
    let counts = &result.counts;
    assert_eq!(counts.review, 2);
    assert_eq!(counts.former, 1);
    assert_eq!(
        counts.drivers + counts.helpers,
        counts.people - counts.office - counts.former
    );
    // Paycom's roster lists everyone else, so nobody else has left.
    assert!(
        result
            .drivers
            .iter()
            .all(|d| d.code == gone || d.role != DriverRole::Former)
    );
}

#[test]
fn links_saved_on_the_meal_break_page_are_kept() {
    dispatch_backend::install();
    let (_root, db, id, _) = ready(Some(json!({
        "revision": 2,
        "links": [{"id":"employee_1","cortexId":"TREYES","paycomCode":"E002"}],
        "separate": ["TTAYLOR"],
    })));
    db.match_drivers(&id).unwrap();
    let result = db.driver_match(&id).unwrap();
    let antonio = code(&result, PAYCOM, "E002");
    assert_eq!(antonio, code(&result, AMAZON, "TREYES"));
    assert_eq!(person(&result, &antonio).link, DriverLink::Saved);
    // Kept apart from Paycom, Taylor is neither joined nor suggested.
    let taylor = code(&result, AMAZON, "TTAYLOR");
    assert_ne!(taylor, code(&result, PAYCOM, "E004"));
    assert_eq!(known_to(&result, &taylor), ["amazon"]);
    assert!(!person(&result, &taylor).review);
}

#[test]
fn the_previous_release_keeps_every_link_after_a_decision() {
    dispatch_backend::install();
    // A link saved for a driver whose data has since gone, beside one Driver Match knows.
    let (_root, db, id, context) = ready(Some(json!({
        "revision": 4,
        "links": [{"id":"employee_1","cortexId":"TGONE","paycomCode":"E007"}],
        "separate": ["TDMITRI"],
    })));
    db.match_drivers(&id).unwrap();
    let before = db.driver_match(&id).unwrap();
    let (antonio, tony) = (
        code(&before, PAYCOM, "E002"),
        code(&before, AMAZON, "TREYES"),
    );
    db.merge_drivers(&context, &tony, &antonio).unwrap();
    let legacy = db
        .dsp(&id)
        .unwrap()
        .setting("employees.provider_links", Value::Null)
        .unwrap();
    assert_eq!(legacy["revision"], 5);
    let pairs: Vec<(&str, &str)> = legacy["links"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| (s(l, "cortexId"), s(l, "paycomCode")))
        .collect();
    assert_eq!(pairs, [("TREYES", "E002"), ("TGONE", "E007")]);
    assert_eq!(legacy["separate"], json!(["TDMITRI"]));
}

/// A roster of the DSP's company as Amazon lists it, each as (transporter ID, name, status,
/// roles), with an email address made of their name, published as the roster's collection
/// would.
fn roster(db: &Store, id: &str, listed: &[(&str, &str, &str, &str)]) {
    db.set_profile(id, json!({"abbreviation":"TST"})).unwrap();
    let request = json!({"collection":"associates","dspAbbreviation":"TST"});
    let collected = dispatch_cortex::COLLECTOR.fixture("UTC", &request).unwrap();
    let mut capture: associates::Capture = serde_json::from_value(collected.data).unwrap();
    capture.photos.clear();
    capture.associates = listed
        .iter()
        .map(|(transporter, name, status, roles)| {
            let email = format!("{}@example.com", name.to_lowercase().replace(' ', "."));
            associates::Associate::read(json!({"transporter_id":transporter,"full_name":name,
                "email_address":email,"operational_status":status,"roles":roles}))
            .unwrap()
            .unwrap()
        })
        .collect();
    db.publish_roster(id, "job-roster", &capture).unwrap();
}

#[test]
fn the_roster_says_who_drives_who_helps_and_who_has_left() {
    dispatch_backend::install();
    let (_root, db, id, _) = ready(None);
    roster(
        &db,
        &id,
        &[
            ("TCASEY", "Casey Rivera", "ACTIVE", "Helper"),
            ("TDANA", "Dana Whitfield", "ACTIVE", "Driver, Helper"),
            ("TPAT", "Pat Quinn", "INACTIVE", "Driver"),
        ],
    );
    db.match_drivers(&id).unwrap();
    let result = db.driver_match(&id).unwrap();
    // The roster's name joins Paycom's employee, and says they help.
    let casey = code(&result, PAYCOM, "E006");
    assert_eq!(casey, code(&result, AMAZON, "TCASEY"));
    assert_eq!(person(&result, &casey).role, DriverRole::Helper);
    // Listed as active, Dana works for the DSP though nothing shows them on a route yet.
    let dana = code(&result, AMAZON, "TDANA");
    assert_eq!(person(&result, &dana).role, DriverRole::Driver);
    // Inactive, unseen and on no other roster, Pat has left.
    let pat = code(&result, AMAZON, "TPAT");
    assert_eq!(person(&result, &pat).role, DriverRole::Former);
}

#[test]
fn a_feature_switched_off_says_nothing_and_its_ids_wait_for_it() {
    dispatch_backend::install();
    let (_root, db, id, context) = ready(None);
    db.match_drivers(&id).unwrap();
    let before = db.driver_match(&id).unwrap();
    let taylor = code(&before, PAYCOM, "E004");
    db.set_feature(&id, "timecard", false, &context.auth.user.id)
        .unwrap();
    // Without Timecard, Paycom names nobody: Taylor is Amazon's driver alone, and nothing
    // new is matched.
    assert_eq!(db.match_drivers(&id).unwrap(), 0);
    let off = db.driver_match(&id).unwrap();
    assert_eq!(off.sources, [AMAZON]);
    assert_eq!(known_to(&off, &taylor), ["amazon"]);
    assert!(off.review.is_empty());
    // Back on, every link is as it was, with nothing to decide again.
    db.set_feature(&id, "timecard", true, &context.auth.user.id)
        .unwrap();
    let on = db.driver_match(&id).unwrap();
    assert_eq!(code(&on, PAYCOM, "E004"), taylor);
    assert_eq!(known_to(&on, &taylor), ["paycom", "amazon"]);
    assert_eq!(on.counts.review, before.counts.review);
}

/// The DSP's Slack workspace `team`, its members each as (member ID, name, email address,
/// whether their account is active), as reading them keeps them.
#[cfg(feature = "slack")]
fn slack(db: &Store, team: &str, id: &str, members: &[(&str, &str, Option<&str>, bool)]) {
    let dsp = db.dsp(id).unwrap();
    dsp.exec(
        "INSERT INTO slack_workspaces(team_id,name,status,scopes,bot_user_id,app_id,connected_at) \
         VALUES (?,'Operations','connected','users:read users:read.email','U0BOT','A0APP',?)",
        [team, "2026-01-01T00:00:00Z"],
    )
    .unwrap();
    for (user, name, email, active) in members {
        dsp.exec(
            "INSERT INTO slack_members(team_id,user_id,name,email,active,guest) \
             VALUES (?,?,?,?,?,0)",
            rusqlite::params![team, user, name, email, active],
        )
        .unwrap();
    }
}

/// Disconnects the DSP's Slack workspace `team`, forgetting its members as disconnecting does.
#[cfg(feature = "slack")]
fn disconnect(db: &Store, id: &str, team: &str) {
    let dsp = db.dsp(id).unwrap();
    for table in ["slack_members", "slack_workspaces"] {
        dsp.exec(&format!("DELETE FROM {table} WHERE team_id=?"), [team])
            .unwrap();
    }
}

#[cfg(feature = "slack")]
#[test]
fn slack_members_join_by_email_and_whom_only_slack_knows_works_in_the_office() {
    dispatch_backend::install();
    let (_root, db, id, _) = ready(None);
    roster(
        &db,
        &id,
        &[
            ("TJORDAN", "Jordan Ellis", "ACTIVE", "Driver"),
            ("TPAT", "Pat Quinn", "INACTIVE", "Driver"),
        ],
    );
    // Slack is none of the DSP's systems until it names someone: no card says "Not in Slack"
    // while a workspace waits for its admin to let Dispatch read its members.
    let slack_ids = DriverSource::parse("slack").unwrap();
    assert!(!db.driver_match(&id).unwrap().sources.contains(&slack_ids));
    slack(
        &db,
        "T0OPS",
        &id,
        &[
            (
                "U0JORDAN",
                "Jordan R Ellis",
                Some("Jordan.Ellis@example.com"),
                true,
            ),
            (
                "U0MORGAN",
                "Morgan Lane",
                Some("morgan.lane@example.com"),
                true,
            ),
            ("U0JESSE", "Jesse Kline", None, false),
            ("U0PAT", "Pat Quinn", Some("pat.quinn@example.com"), true),
        ],
    );
    db.match_drivers(&id).unwrap();
    let result = db.driver_match(&id).unwrap();
    let slack = slack_ids;
    assert!(result.sources.contains(&slack));
    // The roster's email address joins the member to the driver.
    let jordan = code(&result, AMAZON, "TJORDAN");
    assert_eq!(code(&result, slack, "T0OPS:U0JORDAN"), jordan);
    assert_eq!(person(&result, &jordan).role, DriverRole::Driver);
    let member = person(&result, &jordan)
        .ids
        .iter()
        .find(|i| i.source == slack)
        .unwrap();
    assert_eq!(member.linked_by, DriverLink::Email);
    // Someone only Slack knows works in the office while their account is active.
    let morgan = code(&result, slack, "T0OPS:U0MORGAN");
    assert_eq!(known_to(&result, &morgan), ["slack"]);
    assert_eq!(person(&result, &morgan).role, DriverRole::Office);
    let jesse = code(&result, slack, "T0OPS:U0JESSE");
    assert_eq!(person(&result, &jesse).role, DriverRole::Former);
    // Slack keeps nobody working: Pat left Amazon's roster, whatever Slack says.
    let pat = code(&result, AMAZON, "TPAT");
    assert_eq!(code(&result, slack, "T0OPS:U0PAT"), pat);
    assert_eq!(person(&result, &pat).role, DriverRole::Former);
}

/// Someone in two of the DSP's workspaces is one person, holding each workspace's member ID,
/// whichever workspace was connected first and however the workspaces come and go.
#[cfg(feature = "slack")]
#[test]
fn a_member_of_two_workspaces_is_one_person_however_the_workspaces_come_and_go() {
    dispatch_backend::install();
    let (_root, db, id, _) = ready(None);
    roster(&db, &id, &[("TJORDAN", "Jordan Ellis", "ACTIVE", "Driver")]);
    let members = DriverSource::parse("slack").unwrap();
    let operations: &[(&str, &str, Option<&str>, bool)] = &[
        (
            "U0JORDAN",
            "Jordan Ellis",
            Some("jordan.ellis@example.com"),
            true,
        ),
        ("U0LUIS", "Luis Hernandez", Some("luis@example.com"), true),
    ];
    slack(&db, "T0OPS", &id, operations);
    // Their accounts in the other workspace share their email addresses, so neither
    // address nor name is theirs alone among Slack's IDs, but both are among its people.
    slack(
        &db,
        "T0YARD",
        &id,
        &[
            (
                "U0JE",
                "Jordan Ellis",
                Some("Jordan.Ellis@example.com"),
                true,
            ),
            ("U0LH", "Luis Hernandez", Some("luis@example.com"), true),
        ],
    );
    db.match_drivers(&id).unwrap();
    let result = db.driver_match(&id).unwrap();
    let jordan = code(&result, AMAZON, "TJORDAN");
    let luis = code(&result, AMAZON, "TLUIS");
    let accounts = |result: &DriverMatch| {
        [
            ["T0OPS:U0JORDAN", "T0YARD:U0JE"].map(|member| code(result, members, member)),
            ["T0OPS:U0LUIS", "T0YARD:U0LH"].map(|member| code(result, members, member)),
        ]
    };
    let together = [
        [jordan.clone(), jordan.clone()],
        [luis.clone(), luis.clone()],
    ];
    assert_eq!(accounts(&result), together);
    let links: Vec<DriverLink> = person(&result, &jordan)
        .ids
        .iter()
        .filter(|i| i.source == members)
        .map(|i| i.linked_by)
        .collect();
    assert_eq!(links, [DriverLink::Email, DriverLink::Account]);

    // A workspace disconnected leaves its accounts idle, and connected again brings them back
    // to the same people.
    disconnect(&db, &id, "T0OPS");
    db.match_drivers(&id).unwrap();
    let result = db.driver_match(&id).unwrap();
    let yard = ["T0YARD:U0JE", "T0YARD:U0LH"].map(|member| code(&result, members, member));
    assert_eq!(yard, [jordan.clone(), luis.clone()]);
    assert!(
        person(&result, &jordan)
            .ids
            .iter()
            .all(|i| i.id != "T0OPS:U0JORDAN")
    );
    slack(&db, "T0OPS", &id, operations);
    db.match_drivers(&id).unwrap();
    assert_eq!(accounts(&db.driver_match(&id).unwrap()), together);

    // With every workspace gone, Slack is none of the DSP's systems; a workspace connected in
    // their place, with new accounts for the same people, joins them, by an email address or
    // by a name nobody else carries.
    disconnect(&db, &id, "T0OPS");
    disconnect(&db, &id, "T0YARD");
    db.match_drivers(&id).unwrap();
    assert!(!db.driver_match(&id).unwrap().sources.contains(&members));
    slack(
        &db,
        "T0NEW",
        &id,
        &[
            (
                "U0J",
                "Jordan Ellis",
                Some("jordan.ellis@example.com"),
                true,
            ),
            ("U0L", "Luis Hernandez", Some("luis.h@example.com"), true),
        ],
    );
    db.match_drivers(&id).unwrap();
    let result = db.driver_match(&id).unwrap();
    let new = ["T0NEW:U0J", "T0NEW:U0L"].map(|member| code(&result, members, member));
    assert_eq!(new, [jordan, luis]);
}

/// Someone with an account in each of two workspaces, under different email addresses, gets a
/// person for the second, and Driver Match asks whether the two are one: Slack knowing both
/// says nothing, as one person may have several accounts.
#[cfg(feature = "slack")]
#[test]
fn an_account_under_another_address_is_asked_about() {
    dispatch_backend::install();
    let (_root, db, id, _) = ready(None);
    roster(&db, &id, &[("TJORDAN", "Jordan Ellis", "ACTIVE", "Driver")]);
    slack(
        &db,
        "T0OPS",
        &id,
        &[(
            "U0JORDAN",
            "Jordan Ellis",
            Some("jordan.ellis@example.com"),
            true,
        )],
    );
    slack(
        &db,
        "T0YARD",
        &id,
        &[("U0JE", "Jordan Ellis", Some("jordan.e@example.com"), true)],
    );
    db.match_drivers(&id).unwrap();
    let result = db.driver_match(&id).unwrap();
    let jordan = code(&result, AMAZON, "TJORDAN");
    let other = code(
        &result,
        DriverSource::parse("slack").unwrap(),
        "T0YARD:U0JE",
    );
    assert_ne!(other, jordan);
    assert!(
        result
            .review
            .iter()
            .any(|pair| (&pair.person.code, &pair.candidate.code) == (&other, &jordan))
    );
}

/// A phone number that isn't North American matches nobody, and is still shown.
#[cfg(feature = "slack")]
#[test]
fn a_phone_number_nobody_is_matched_by_is_still_shown() {
    dispatch_backend::install();
    let (_root, db, id, _) = ready(None);
    slack(
        &db,
        "T0OPS",
        &id,
        &[(
            "U0MORGAN",
            "Morgan Lane",
            Some("morgan.lane@example.com"),
            true,
        )],
    );
    db.dsp(&id)
        .unwrap()
        .exec("UPDATE slack_members SET phone='+44 20 7946 0958'", [])
        .unwrap();
    db.match_drivers(&id).unwrap();
    let result = db.driver_match(&id).unwrap();
    let members = DriverSource::parse("slack").unwrap();
    let morgan = code(&result, members, "T0OPS:U0MORGAN");
    let shown: Vec<&str> = person(&result, &morgan)
        .ids
        .iter()
        .flat_map(|i| i.phones.iter().map(String::as_str))
        .collect();
    assert_eq!(shown, ["+44 20 7946 0958"]);
}
