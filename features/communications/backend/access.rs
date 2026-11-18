//! Who may use one line's texting, as each route lets a member in and as it checks again after
//! each wait outside the database: by the route's own access, and on another line only while
//! Dispatch Phone is on for the DSP too, which that line's own permission doesn't say, as its
//! phones' texts are Dispatch Phone's to keep.
use super::phone::storage::Line;
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
    /// not found, on another line while Dispatch Phone is off.
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

/// Refuses the line `line`, as not found, to the member `c` while Dispatch Phone is off for the DSP
/// and it's another line's: its own permission still lets them in.
pub fn served(c: &Context, line: Option<&str>) -> Result<()> {
    ensure(
        line.is_none() || c.has(crate::FEATURE.switch.id),
        "not_found",
        404,
    )
}
