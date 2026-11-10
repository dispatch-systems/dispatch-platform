//! A DSP's beta testers: those of its people the platform owner chooses to see what is in beta
//! there, and to hold all of it, whatever their roles. Each DSP keeps its own in its directory,
//! where each goes with their account. Who they are is the platform's record, never the DSP's.
use crate::{
    Error, Result,
    db::{Store, iso},
    ensure,
    platform_owner::api::types::{BetaMember, DspBetaTesters},
};
use std::collections::BTreeMap;

impl Store {
    /// Whether one of the DSP's people is one of its beta testers.
    pub fn beta_tester(&self, dsp: &str, user: &str) -> Result<bool> {
        Ok(self
            .dsp(dsp)?
            .one("SELECT 1 FROM beta_testers WHERE user_id=?", [user])?
            .is_some())
    }
    /// The DSP's people, its beta testers among them.
    pub fn beta_testers(&self, dsp: &str) -> Result<DspBetaTesters> {
        self.find_dsp(dsp)?;
        let since: BTreeMap<String, String> = self
            .dsp(dsp)?
            .query_as("SELECT user_id,added_at FROM beta_testers", [])?
            .into_iter()
            .collect();
        Ok(DspBetaTesters {
            members: self
                .members(dsp)?
                .into_iter()
                .map(|member| BetaMember {
                    tester_since: since.get(&member.user_id).cloned(),
                    id: member.user_id,
                    name: member.name,
                    email: member.email,
                    role: member.role,
                })
                .collect(),
        })
    }
    /// Makes some of the DSP's people its beta testers; those who are already stay as they
    /// were. Audited, each on their own; open views expire.
    pub fn add_beta_testers(
        &self,
        dsp: &str,
        users: &[String],
        actor: &str,
    ) -> Result<DspBetaTesters> {
        self.change_beta_testers(dsp, users, actor, true)
    }
    /// Stops one of the DSP's people testing what is in beta there.
    pub fn remove_beta_tester(&self, dsp: &str, user: &str, actor: &str) -> Result<DspBetaTesters> {
        self.change_beta_testers(dsp, &[user.to_owned()], actor, false)
    }
    fn change_beta_testers(
        &self,
        dsp: &str,
        users: &[String],
        actor: &str,
        testing: bool,
    ) -> Result<DspBetaTesters> {
        self.find_dsp(dsp)?;
        let people = self.dsp(dsp)?;
        let members = self.members(dsp)?;
        let named: Vec<_> = users
            .iter()
            .map(|user| {
                members
                    .iter()
                    .find(|member| &member.user_id == user)
                    .ok_or_else(|| Error::new("member_not_found", 404))
            })
            .collect::<Result<_>>()?;
        ensure(!named.is_empty(), "invalid_input", 400)?;
        self.across(&people, || {
            let mut changed = false;
            for member in &named {
                let was = people
                    .one(
                        "SELECT 1 FROM beta_testers WHERE user_id=?",
                        [&member.user_id],
                    )?
                    .is_some();
                if was == testing {
                    continue;
                }
                if testing {
                    people.exec(
                        "INSERT INTO beta_testers(user_id,added_at) VALUES (?,?)",
                        [&member.user_id, &iso()],
                    )?;
                } else {
                    people.exec(
                        "DELETE FROM beta_testers WHERE user_id=?",
                        [&member.user_id],
                    )?;
                }
                self.audit_ref(
                    Some(actor),
                    Some(dsp),
                    if testing {
                        "dsp.beta_tester_added"
                    } else {
                        "dsp.beta_tester_removed"
                    },
                    "",
                    Some(&member.name),
                    &[],
                    Some(("member", &member.user_id)),
                )?;
                changed = true;
            }
            if changed {
                self.expire_views(dsp)?;
            }
            Ok(())
        })?;
        self.beta_testers(dsp)
    }
}
