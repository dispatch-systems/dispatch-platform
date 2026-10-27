//! Whom a group message can go to: every active member of the DSP's workspaces where
//! Communications works, each person once, as Driver Match tells them apart, with their accounts
//! there, the one last written in first; and the groups to choose from, each
//! holding someone. Today: everyone on a route today, then each wave by when it starts, as Routes
//! has read the DSP's own day (`driver_routes`), while it's on and has read some. Roles: drivers,
//! helpers and the office, as Driver Match knows them. Workspaces, with two or more where
//! Communications works: everyone, and each workspace's members.
use super::GroupChoiceKind;
use crate::{
    api::types::{GroupChoice, GroupChoices, GroupMember, Reachable},
    backend::{people::People, storage::CommunicationsStore},
    integrations::messages::ID,
};
use dispatch_core::{
    Error, Result, State,
    db::Store,
    foundation::{clock, names},
    manifest::people::{self, PersonRole},
};
use dispatch_routes_contract as routes;
use dispatch_slack::kit;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Someone the DSP can message, by their key: their role, if Driver Match knows it, and each of
/// their accounts: its workspace, their ID there and the name Slack gives them.
type Reached = (Option<PersonRole>, Vec<(String, String, String)>);
/// The roles a group can be, in the order offered: never those who have left.
const ROLES: [PersonRole; 3] = [PersonRole::Driver, PersonRole::Helper, PersonRole::Office];

/// Whom the DSP can message together, and the groups to choose from, today being the day it is
/// in `zone`, the DSP's timezone.
pub fn choices(db: &Store, state: &State, dsp: &str, zone: &str) -> Result<GroupChoices> {
    let people = People::read(db, state, dsp)?;
    let everyone = people.everyone();
    let on = kit::workspaces_on(db, dsp, ID)?;
    // Workspaces in the order the DSP's are listed, by name.
    let order: Vec<String> = kit::workspaces(db, dsp)?
        .into_iter()
        .map(|workspace| workspace.team)
        .filter(|team| on.contains(team))
        .collect();
    let mut last_written: HashMap<(String, String), i64> = HashMap::new();
    for team in &order {
        for conversation in db.communications_in(dsp, team)? {
            if let Some(at) = conversation.last_at {
                last_written.insert((conversation.team, conversation.user), at);
            }
        }
    }
    // Each person once, by their person's code, else their account; their role, if known.
    let mut reachable: BTreeMap<String, Reached> = BTreeMap::new();
    for (team, member) in kit::members(db, dsp)? {
        if !member.active || !order.contains(&team) {
            continue;
        }
        let person = everyone.holding(kit::SOURCE, &kit::member_id(&team, &member.user));
        let key = person.map_or_else(
            || kit::member_id(&team, &member.user),
            |person| person.code.clone(),
        );
        let entry = reachable
            .entry(key)
            .or_insert_with(|| (person.map(|person| person.role), Vec::new()));
        entry.1.push((team, member.user, member.name));
    }
    let rank = |team: &str| order.iter().position(|each| each == team);
    let mut listed: Vec<Reachable> = reachable
        .iter()
        .map(|(key, (_, accounts))| {
            let mut accounts = accounts.clone();
            // The one last written in first, then by workspace.
            accounts.sort_by(|a, b| {
                let wrote = |(team, user, _): &(String, String, String)| {
                    last_written.get(&(team.clone(), user.clone())).copied()
                };
                wrote(b)
                    .cmp(&wrote(a))
                    .then_with(|| rank(&a.0).cmp(&rank(&b.0)))
                    .then_with(|| a.1.cmp(&b.1))
            });
            let (team, user, name) = &accounts[0];
            Reachable {
                key: key.clone(),
                person: people.correspondent(team, user, name),
                members: accounts
                    .iter()
                    .map(|(team, user, _)| GroupMember {
                        team_id: team.clone(),
                        user_id: user.clone(),
                    })
                    .collect(),
            }
        })
        .collect();
    listed.sort_by(|a, b| names::compare(&a.person.name, &b.person.name));
    let keys = |held: &dyn Fn(&Reachable) -> bool| -> Vec<String> {
        listed
            .iter()
            .filter(|each| held(each))
            .map(|each| each.key.clone())
            .collect()
    };

    let mut today = Vec::new();
    if let Some((driving, waves)) = on_routes(db, dsp, zone, everyone)? {
        today.push(choice(
            GroupChoiceKind::Route,
            keys(&|each| driving.contains(&each.key)),
        ));
        for (at, codes) in waves {
            today.push(GroupChoice {
                wave_at: Some(at),
                ..choice(
                    GroupChoiceKind::Wave,
                    keys(&|each| codes.contains(&each.key)),
                )
            });
        }
    }
    let roles = ROLES
        .iter()
        .map(|role| GroupChoice {
            role: Some(*role),
            ..choice(
                GroupChoiceKind::Role,
                keys(&|each| {
                    reachable
                        .get(&each.key)
                        .is_some_and(|(of, _)| *of == Some(*role))
                }),
            )
        })
        .collect();
    let mut workspaces = Vec::new();
    if order.len() > 1 {
        workspaces.push(choice(GroupChoiceKind::Everyone, keys(&|_| true)));
        for team in &order {
            workspaces.push(GroupChoice {
                team_id: Some(team.clone()),
                ..choice(
                    GroupChoiceKind::Workspace,
                    keys(&|each| each.members.iter().any(|member| member.team_id == *team)),
                )
            });
        }
    }
    let held = |choices: Vec<GroupChoice>| -> Vec<GroupChoice> {
        choices
            .into_iter()
            .filter(|choice| !choice.people.is_empty())
            .collect()
    };
    Ok(GroupChoices {
        people: listed,
        today: held(today),
        roles: held(roles),
        workspaces: held(workspaces),
    })
}

/// A group to choose from of `kind`, holding the people `people`.
fn choice(kind: GroupChoiceKind, people: Vec<String>) -> GroupChoice {
    GroupChoice {
        kind,
        wave_at: None,
        role: None,
        team_id: None,
        people,
    }
}

/// Everyone on a route today, by their person's code, and each wave of them by when it starts,
/// the first first: none while Routes is off or has read nothing of today.
type Waves = (BTreeSet<String>, BTreeMap<i64, BTreeSet<String>>);
fn on_routes(
    db: &Store,
    dsp: &str,
    zone: &str,
    everyone: &people::Everyone,
) -> Result<Option<Waves>> {
    let Some(reader) = routes::for_dsp(db, dsp)? else {
        return Ok(None);
    };
    let zone: chrono_tz::Tz = zone
        .parse()
        .map_err(|_| Error::new("invalid_timezone", 400))?;
    let day = clock::today(zone).to_string();
    let driven = reader.day(db, dsp, &day)?;
    let (mut driving, mut waves) = (BTreeSet::new(), BTreeMap::new());
    for route in driven.routes {
        let Some(person) = everyone.holding(reader.source(), &route.driver_id) else {
            continue;
        };
        driving.insert(person.code.clone());
        if let Some(at) = route.wave_start_at {
            waves
                .entry(at)
                .or_insert_with(BTreeSet::new)
                .insert(person.code.clone());
        }
    }
    Ok((!driving.is_empty()).then_some((driving, waves)))
}
