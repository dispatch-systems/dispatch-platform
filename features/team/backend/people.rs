//! The DSP's members, as Driver Match reads them: each by their account, with its name and
//! email address. Members are people the DSP can reach, not whom it employs: an office
//! member's sources may know them, and a member nothing else knows works in the office.
use dispatch_core::{
    Result,
    db::Store,
    manifest::people::{KindInfo, MEMBERS, Named, People},
};

/// Everyone who can sign in to the DSP, by the name and email address on their account.
pub const PEOPLE: People = People::new(
    KindInfo {
        id: "members",
        order: 80,
        label: "Dispatch",
        icon: "UserRound",
    },
    MEMBERS,
    members,
)
.contacts_only();

fn members(store: &Store, dsp: &str) -> Result<Vec<Named>> {
    Ok(store
        .members(dsp)?
        .into_iter()
        .map(|member| Named {
            name: member.name,
            listed: true,
            emails: vec![member.email],
            ..Named::new(&member.user_id)
        })
        .collect())
}
