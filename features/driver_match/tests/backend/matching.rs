use super::*;
use dispatch_core::manifest::people::SourceData;

/// The sources these plans name, as their owners declare them.
static PAYCOM_IDS: SourceData = SourceData {
    id: "paycom",
    label: "Paycom",
    order: 10,
};
static AMAZON_IDS: SourceData = SourceData {
    id: "amazon",
    label: "Amazon",
    order: 20,
};
static SLACK_IDS: SourceData = SourceData {
    id: "slack",
    label: "Slack",
    order: 30,
};
const PAYCOM_SOURCE: DriverSource = DriverSource::new(&PAYCOM_IDS);
const AMAZON_SOURCE: DriverSource = DriverSource::new(&AMAZON_IDS);
const SLACK_SOURCE: DriverSource = DriverSource::new(&SLACK_IDS);

fn id(source: DriverSource, id: &str, names: &[&str]) -> Identity {
    Identity {
        names: names.iter().map(|n| (*n).to_owned()).collect(),
        ..Identity::new(source, id)
    }
}
fn paycom(code: &str, name: &str) -> Identity {
    id(PAYCOM_SOURCE, code, &[name])
}
fn amazon(transporter: &str, names: &[&str]) -> Identity {
    id(AMAZON_SOURCE, transporter, names)
}
fn slack(member: &str, name: &str) -> Identity {
    id(SLACK_SOURCE, member, &[name])
}
/// `x` with an email address and a phone number, as its source wrote them.
fn reached(mut x: Identity, email: &str, phone: &str) -> Identity {
    x.contacts.add(Contact::Email, email);
    x.contacts.add(Contact::Phone, phone);
    x
}
/// `x`, which its source says is the same person as `others`.
fn same_as(mut x: Identity, others: &[&str]) -> Identity {
    x.same_as = others.iter().map(|other| (*other).to_owned()).collect();
    x
}
/// Plans from nobody, and answers who each ID ends up with, by how.
fn run(incoming: &[Identity]) -> BTreeMap<String, (Target, DriverLink)> {
    plan(BTreeMap::new(), incoming)
        .into_iter()
        .map(|d| (incoming[d.index].id.clone(), (d.target, d.link)))
        .collect()
}
fn together(result: &BTreeMap<String, (Target, DriverLink)>, a: &str, b: &str) -> bool {
    result[a].0 == result[b].0
}
/// Where each of `incoming` goes, given who holds what already, and how.
fn decided(holders: &[(&str, Vec<Identity>)], incoming: &[Identity]) -> Vec<(Target, DriverLink)> {
    let holders = holders
        .iter()
        .map(|(code, ids)| ((*code).to_owned(), ids.clone()))
        .collect();
    plan(holders, incoming)
        .into_iter()
        .map(|d| (d.target, d.link))
        .collect()
}
fn person(code: &str) -> Target {
    Target::Person(code.to_owned())
}

#[test]
fn unique_names_join_across_formats_and_variants_need_the_whole_surname() {
    let result = run(&[
        paycom("A001", "REYES, ANTONIO"),
        paycom("A002", "HERNANDEZ ORTIZ, LUIS"),
        paycom("A003", "WHITFIELD, JAMES"),
        amazon("T1", &["Antonio Reyes"]),
        amazon("T2", &["Luis Hernandez"]),
        amazon("T3", &["James Whitfield Jr"]),
    ]);
    assert!(together(&result, "A001", "T1"));
    assert_eq!(result["T1"].1, DriverLink::Name);
    assert!(together(&result, "A002", "T2"));
    assert_eq!(result["T2"].1, DriverLink::Variant);
    assert!(together(&result, "A003", "T3"));
    assert_eq!(result["A001"].1, DriverLink::New);
}
#[test]
fn any_spelling_amazon_knows_can_match_and_middle_names_are_looked_past() {
    let result = run(&[
        paycom("A001", "OKAFOR, DANIEL"),
        // The scorecard adds a middle name; the routes name is plain.
        amazon("T1", &["Daniel Okafor", "Daniel Chukwuma Okafor"]),
        paycom("A002", "BELL, MARCUS"),
        amazon("T2", &["Marcus Andre Bell"]),
        // Paycom lists the middle name they go by.
        paycom("A003", "MORGAN, RUBEN"),
        amazon("T3", &["Martin Morgan", "Martin Ruben Morgan"]),
    ]);
    assert!(together(&result, "A001", "T1"));
    assert!(together(&result, "A002", "T2"));
    assert_eq!(result["T2"].1, DriverLink::Variant);
    assert!(together(&result, "A003", "T3"));
    assert_eq!(result["T3"].1, DriverLink::Variant);
}
#[test]
fn a_middle_name_two_people_could_go_by_is_never_guessed() {
    let result = run(&[
        paycom("A001", "GARCIA, JOSE"),
        paycom("A002", "GARCIA, JUAN"),
        amazon("T1", &["Juan Jose Garcia"]),
    ]);
    assert_eq!(result["T1"].1, DriverLink::New);
}
#[test]
fn shared_names_and_different_first_names_are_never_guessed() {
    let result = run(&[
        paycom("A001", "SMITH, JOHN"),
        paycom("A002", "SMITH, JOHN"),
        amazon("T1", &["John Smith"]),
        paycom("A003", "LEE, MARIA"),
        amazon("T2", &["Maria Lee"]),
        amazon("T3", &["Maria Lee"]),
        paycom("A004", "REYES, ANTONIO"),
        amazon("T4", &["Tony Reyes"]),
    ]);
    for (a, b) in [
        ("A001", "T1"),
        ("A002", "T1"),
        ("A003", "T2"),
        ("A003", "T3"),
        ("A004", "T4"),
    ] {
        assert!(!together(&result, a, b), "{a} and {b}");
    }
    assert_eq!(result["T4"].1, DriverLink::New);
}
#[test]
fn a_person_who_has_an_amazon_id_takes_no_second_by_name_or_contact() {
    let devon = vec![
        reached(paycom("A001", "CARTER, DEVON"), "", "6125550142"),
        amazon("T1", &["Devon Carter"]),
    ];
    let incoming = [reached(amazon("T9", &["Devon Carter"]), "", "6125550142")];
    assert_eq!(
        decided(&[("K7M2QX", devon)], &incoming),
        [(Target::New(0), DriverLink::New)]
    );
}
#[test]
fn amazon_seen_first_is_joined_by_paycom_later() {
    let priya = vec![amazon("T1", &["Priya Natarajan"])];
    let incoming = [paycom("A001", "NATARAJAN, PRIYA")];
    assert_eq!(
        decided(&[("W5HC2K", priya)], &incoming),
        [(person("W5HC2K"), DriverLink::Name)]
    );
}
#[test]
fn a_contact_only_two_people_share_joins_them_when_their_names_agree() {
    let result = run(&[
        paycom("A001", "HOLLOWAY, ANDRE"),
        reached(
            amazon("T1", &["Andre Holloway"]),
            "dre@example.com",
            "6125550148",
        ),
        // Slack's own name for them: a nickname the matcher doesn't know.
        reached(slack("U1", "Dre Holloway"), "dre@example.com", ""),
        // The same phone, but a name that isn't theirs.
        reached(slack("U2", "Front Desk"), "", "6125550148"),
        // Its phone is Amazon's too, but nobody shares the name: still Slack's own.
        reached(amazon("T2", &["Priya Natarajan"]), "", "6125550199"),
        reached(slack("U3", "P N"), "", "6125550199"),
    ]);
    assert!(together(&result, "A001", "T1"));
    assert!(together(&result, "T1", "U1"));
    assert_eq!(result["U1"].1, DriverLink::Email);
    assert!(!together(&result, "T1", "U2"));
    assert!(!together(&result, "T2", "U3"));
    assert_eq!(result["U3"].1, DriverLink::New);
}
#[test]
fn a_phone_only_the_two_share_joins_them_when_a_first_name_is_all_their_names_share() {
    let result = run(&[
        reached(paycom("A001", "OKAFOR, SHANICE"), "", "6125550177"),
        // Married since, under a new last name.
        reached(amazon("T1", &["Shanice Bell"]), "", "6125550177"),
    ]);
    assert!(together(&result, "A001", "T1"));
    assert_eq!(result["T1"].1, DriverLink::Phone);
}
#[test]
fn a_name_written_alike_decides_before_a_variant_could_guess() {
    let result = run(&[
        paycom("A001", "REYES, MARIA"),
        paycom("A002", "REYES LOPEZ, MARIA"),
        amazon("T1", &["Maria Reyes"]),
    ]);
    // "Maria Reyes" is A001's name as written, though it is a variant of A002's too.
    assert!(together(&result, "A001", "T1"));
    assert_eq!(result["T1"].1, DriverLink::Name);
}
#[test]
fn a_contact_two_ids_of_one_source_share_says_nothing() {
    let result = run(&[
        // A family's phone, on two of Amazon's associates.
        reached(amazon("T1", &["Sam Rivera"]), "", "6125550123"),
        reached(amazon("T2", &["Alex Rivera"]), "", "6125550123"),
        reached(slack("U1", "Sam Rivera"), "", "6125550123"),
    ]);
    // Not by the phone; by the one name nobody else carries.
    assert!(together(&result, "T1", "U1"));
    assert_eq!(result["U1"].1, DriverLink::Name);
    assert!(!together(&result, "T2", "U1"));
}
#[test]
fn another_account_joins_only_its_one_holder_whose_names_agree() {
    let holders = [
        (
            "K7M2QX",
            vec![
                paycom("A001", "CARTER, DEVON"),
                slack("T1:U1", "Devon Carter"),
            ],
        ),
        // Their account in another workspace, split from them before.
        ("W5HC2K", vec![slack("T2:U2", "Devon Carter")]),
        // An address a front desk shares across workspaces.
        ("R39CFT", vec![slack("T1:U3", "Front Desk")]),
    ];
    let incoming = [
        same_as(slack("T3:U1", "Devon Carter"), &["T1:U1", "T2:U2"]),
        same_as(slack("T3:U3", "Priya Natarajan"), &["T1:U3"]),
    ];
    // A person decides both.
    assert_eq!(
        decided(&holders, &incoming),
        [
            (Target::New(0), DriverLink::New),
            (Target::New(1), DriverLink::New)
        ]
    );
}
