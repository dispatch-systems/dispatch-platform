//! What a member sees and saves of a DSP's Settings: the sections of features on for the DSP
//! under permissions they hold, on the pages those sections are on. Which DSP's and whose
//! values they are, over HTTP, is `integration/isolation.rs`'s.
use super::*;
use crate::{
    accounts::{Auth, api::types::PublicUser},
    foundation::config::Site,
    manifest::{
        feature, mandatory, optional, perm,
        settings::{Field, number, section},
    },
    testing,
};
use serde_json::{Value, json};

const DAYS: &[Field] = &[number("days", "Keep", 30, 3650, Some(90))];
/// On for every DSP, as the one feature core's own registry has.
static ON: Feature = Feature {
    switch: mandatory("stand_in", "Stand-in"),
    permissions: &[perm("stand_in.manage", "Manage Stand-in", 1)],
    settings: &[
        section("stand_in.policy", "Policy", "stand_in.manage", DAYS).order(20),
        section("stand_in.part", "Part", "stand_in.manage", DAYS).part("stand_in.extra"),
        section("stand_in.mine", "Mine", roles::ACCESS, DAYS)
            .member()
            .on("general")
            .order(5),
        section("stand_in.limits", "Limits", roles::ACCESS, DAYS)
            .on("data")
            .keywords(&["window"])
            .found(named)
            .order(10),
    ],
    ..feature("stand_in")
};
/// Off for every DSP.
static OFF: Feature = Feature {
    switch: optional("switched_off", "Switched off", &[]),
    settings: &[section(
        "switched_off.policy",
        "Policy",
        roles::ACCESS,
        DAYS,
    )],
    ..feature("switched_off")
};
/// Found by the name of the DSP the member reads it in.
fn named(c: &Context) -> Vec<String> {
    vec![c.dsp.name.clone()]
}
fn declared() -> Vec<Declared> {
    [&ON, &OFF]
        .into_iter()
        .flat_map(|feature| feature.settings.iter().map(move |s| (Some(feature), s)))
        .collect()
}

struct Seeded {
    _root: tempfile::TempDir,
    db: Store,
    northline: String,
}
fn seeded() -> Seeded {
    testing::install(&[], &[]);
    let (_root, db) = testing::seeded();
    let dsp = |name: &str| {
        let row = db.platform.one("SELECT id FROM dsps WHERE name=?", [name]);
        crate::db::s(&row.unwrap().unwrap(), "id").to_owned()
    };
    let northline = dsp("Northline Logistics");
    Seeded {
        _root,
        db,
        northline,
    }
}
fn auth(user: PublicUser, scope: Option<&str>, site: Site) -> Auth {
    Auth {
        user,
        hash: String::new(),
        csrf: String::new(),
        raw: String::new(),
        preview: None,
        site,
        scope: scope.map(str::to_owned),
    }
}
impl Seeded {
    /// The platform owner, who sees `dsp` as its owners do and more.
    fn owner(&self, dsp: &str) -> Context {
        let user = self
            .db
            .platform
            .one_as("SELECT * FROM users WHERE platform_owner=1", []);
        let auth = auth(user.unwrap().unwrap(), None, Site::Admin);
        self.db.context(&auth, dsp, roles::ACCESS).unwrap()
    }
    /// Northline's member, whose role holds no permission.
    fn member(&self) -> Context {
        let people = self.db.dsp(&self.northline).unwrap();
        let user = people.one_as(
            "SELECT * FROM users WHERE email=?",
            ["member@dispatch.test"],
        );
        let auth = auth(
            user.unwrap().unwrap(),
            Some(&self.northline),
            Site::Dsp("nll".into()),
        );
        drop(people);
        self.db
            .context(&auth, &self.northline, roles::ACCESS)
            .unwrap()
    }
    fn sees(&self, c: &Context) -> (Vec<String>, Vec<String>) {
        let answer = sections(&self.db, c, declared()).unwrap();
        (
            answer.pages.iter().map(|page| page.id.clone()).collect(),
            answer.sections.iter().map(|s| s.id.clone()).collect(),
        )
    }
    fn save(&self, c: &Context, id: &str, body: Value) -> Result<SettingsSaved> {
        save_as(&self.db, c, declared(), id, &body)
    }
}
fn refused(saved: Result<SettingsSaved>) -> (String, u16) {
    let error = saved.expect_err("refused");
    (error.code, error.status)
}

#[test]
fn a_member_sees_the_sections_of_features_on_whose_permissions_they_hold() {
    let at = seeded();
    // Personal pages first, then the DSP's, then each feature's own; on each, by place.
    let pages = |ids: &[&str]| ids.iter().map(|id| (*id).to_owned()).collect::<Vec<_>>();
    assert_eq!(
        at.sees(&at.owner(&at.northline)),
        (
            pages(&["general", "data", "stand-in"]),
            pages(&["stand_in.mine", "stand_in.limits", "stand_in.policy"])
        )
    );
    assert_eq!(
        at.sees(&at.member()),
        (
            pages(&["general", "data"]),
            pages(&["stand_in.mine", "stand_in.limits"])
        )
    );
    let answer = sections(&at.db, &at.member(), declared()).unwrap();
    let own = sections(&at.db, &at.owner(&at.northline), declared()).unwrap();
    let page = own.pages.iter().find(|page| page.id == "stand-in").unwrap();
    assert_eq!(
        (page.label.as_str(), page.group),
        ("Stand-in", SettingsGroup::Features)
    );
    assert_eq!(answer.pages[0].label, "Profile");
    // Search finds a section by its words and by those its DSP lists for it.
    assert_eq!(
        answer.sections[1].keywords,
        ["window", "Northline Logistics"]
    );

    // Saving one asks the same: shown, then permitted.
    let body = json!({"values": {"days": 120}, "revision": 0});
    let member = at.member();
    assert_eq!(
        refused(at.save(&member, "stand_in.policy", body.clone())),
        ("permission_denied".into(), 403)
    );
    for hidden in ["switched_off.policy", "stand_in.part", "stand_in.nothing"] {
        assert_eq!(
            refused(at.save(&at.owner(&at.northline), hidden, body.clone())),
            ("not_found".into(), 404)
        );
    }
    assert_eq!(
        refused(at.save(&member, "stand_in.limits", json!({"values": {"days": 120}}))),
        ("invalid_input".into(), 400)
    );
    let saved = at.save(&member, "stand_in.limits", body).unwrap();
    let SettingsSaved::Section { section } = saved else {
        panic!("saved")
    };
    assert_eq!(
        (section.revision, &section.values["days"]),
        (1, &json!(120))
    );
}
