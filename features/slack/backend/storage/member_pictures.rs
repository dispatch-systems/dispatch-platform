//! The profile pictures kept of the members of a DSP's connected workspaces.
use dispatch_core::{Result, db::Store, foundation::crypto};
use rusqlite::params;
use std::collections::BTreeMap;

pub trait MemberPictureStore {
    /// The address each of the workspace's members' pictures came from, by member.
    fn slack_member_picture_sources(
        &self,
        dsp: &str,
        team: &str,
    ) -> Result<BTreeMap<String, String>>;
    /// The version of each picture kept of the DSP's members still kept, by workspace and member.
    fn slack_member_picture_versions(
        &self,
        dsp: &str,
    ) -> Result<BTreeMap<(String, String), String>>;
    /// The picture kept of the workspace `team`'s member `user`, while they're a member kept.
    fn slack_member_picture(&self, dsp: &str, team: &str, user: &str) -> Result<Option<Vec<u8>>>;
    /// Keeps the member's picture, from `source`, in place of the one they had, while they're a
    /// member kept: its version is its picture's digest.
    fn keep_slack_member_picture(
        &self,
        dsp: &str,
        team: &str,
        user: &str,
        source: &str,
        picture: &[u8],
    ) -> Result<()>;
    /// Forgets the member's picture, as they no longer set one of their own.
    fn drop_slack_member_picture(&self, dsp: &str, team: &str, user: &str) -> Result<()>;
}

impl MemberPictureStore for Store {
    fn slack_member_picture_sources(
        &self,
        dsp: &str,
        team: &str,
    ) -> Result<BTreeMap<String, String>> {
        let rows: Vec<(String, String)> = self.dsp(dsp)?.query_as(
            "SELECT user_id,source FROM slack_member_pictures WHERE team_id=?",
            [team],
        )?;
        Ok(rows.into_iter().collect())
    }
    fn slack_member_picture_versions(
        &self,
        dsp: &str,
    ) -> Result<BTreeMap<(String, String), String>> {
        let rows: Vec<(String, String, String)> = self.dsp(dsp)?.query_as(
            "SELECT p.team_id,p.user_id,p.version FROM slack_member_pictures p \
             JOIN slack_members m ON m.team_id=p.team_id AND m.user_id=p.user_id",
            [],
        )?;
        Ok(rows
            .into_iter()
            .map(|(team, user, version)| ((team, user), version))
            .collect())
    }
    fn slack_member_picture(&self, dsp: &str, team: &str, user: &str) -> Result<Option<Vec<u8>>> {
        let found: Option<(Vec<u8>,)> = self.dsp(dsp)?.one_as(
            "SELECT p.picture FROM slack_member_pictures p JOIN slack_members m \
             ON m.team_id=p.team_id AND m.user_id=p.user_id WHERE p.team_id=? AND p.user_id=?",
            [team, user],
        )?;
        Ok(found.map(|(picture,)| picture))
    }
    fn keep_slack_member_picture(
        &self,
        dsp: &str,
        team: &str,
        user: &str,
        source: &str,
        picture: &[u8],
    ) -> Result<()> {
        let version = &crypto::sha(picture)[..16];
        self.dsp(dsp)?.exec(
            "INSERT OR REPLACE INTO slack_member_pictures (team_id,user_id,source,version,picture) \
             SELECT ?1,?2,?3,?4,?5 WHERE EXISTS \
             (SELECT 1 FROM slack_members WHERE team_id=?1 AND user_id=?2)",
            params![team, user, source, version, picture],
        )?;
        Ok(())
    }
    fn drop_slack_member_picture(&self, dsp: &str, team: &str, user: &str) -> Result<()> {
        self.dsp(dsp)?.exec(
            "DELETE FROM slack_member_pictures WHERE team_id=? AND user_id=?",
            [team, user],
        )?;
        Ok(())
    }
}
