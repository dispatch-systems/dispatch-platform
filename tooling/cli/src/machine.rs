//! The machine `dispatchdev` runs on, by what it runs itself beside GitHub.

/// A machine `dispatchdev` runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Machine {
    /// dispatch-dev, and CI's runners: the local checks and the previews run here.
    Linux,
    /// The owner's MacBook: neither runs here. GitHub runs every check, and a change is tried
    /// on Dev once it ships.
    Mac,
}
impl Machine {
    /// This machine, by the OS it was built for, never by whether it holds Dev: CI's Linux
    /// runners hold none, and run the local checks all the same.
    pub const THIS: Self = if cfg!(target_os = "linux") {
        Self::Linux
    } else {
        Self::Mac
    };
    /// Whether the local checks run here. Their rules, lint and clippy cover the Linux hosts'
    /// own code, which macOS fails: the host manager swaps files with `renameat2`, the ops
    /// tests expect a temporary folder that isn't macOS's symlinked `/private/tmp`, and the
    /// server multiplies `statvfs` fields that macOS types otherwise.
    pub const fn checks(self) -> bool {
        matches!(self, Self::Linux)
    }
    /// Whether previews run here: each is a systemd user service, `dispatch-preview@<name>`,
    /// logging to journald as Dev does.
    pub const fn previews(self) -> bool {
        matches!(self, Self::Linux)
    }
}
