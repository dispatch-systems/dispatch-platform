//! Who may use one line's texting, as each route lets a member in and as it checks again after
//! each wait outside the database: by the route's own access, while the line's own feature is on
//! for the DSP and in their sight, which managing all its connections doesn't say. Another line
//! never answers to Dispatch Phone's switch, nor Dispatch Phone's to another's.
use super::{lines, phone::storage::Line};
use dispatch_core::{
    Result,
    accounts::Context,
    db::Store,
    ensure,
    server::http::{
        input::Input,
        route::{Dsp, Grant},
    },
};

/// A route's access to the texting of one line: Dispatch Phone's own (none), or another feature's,
/// by its feature's name.
#[derive(Clone, Copy)]
pub struct Access {
    pub grant: Dsp,
    pub line: Option<&'static str>,
}
impl Access {
    /// The texting of `line`, Dispatch Phone's own for none, as `grant` lets a member in.
    pub fn of(grant: Dsp, line: Option<Line<'static>>) -> Self {
        Self {
            grant,
            line: line.map(|line| line.id),
        }
    }
    /// The member `c`, as their access was just read, while they may use the line: refused, as
    /// not found, while its feature is off or out of their sight.
    pub fn admit(self, c: Context) -> Result<Context> {
        served(&c, self.line)?;
        Ok(c)
    }
    /// Who asks `input`, while they may use the line.
    pub fn authorize(self, db: &Store, input: &Input) -> Result<Context> {
        self.admit(self.grant.authorize(db, input)?)
    }
    /// Checks again, after work done outside the database, that the member `c` may still use the
    /// line.
    pub fn revalidate(self, db: &Store, c: &Context) -> Result<Context> {
        self.admit(self.grant.revalidate(db, c)?)
    }
}

/// Refuses the line `line`, as not found, to the member `c` while its own feature, Dispatch Phone
/// for its own, is off for the DSP or out of their sight: managing all the DSP's connections still
/// lets them in.
pub fn served(c: &Context, line: Option<&str>) -> Result<()> {
    ensure(c.has(lines::switch(line)), "not_found", 404)
}
