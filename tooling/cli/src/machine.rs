//! The machine `dispatchdev` runs on, by whether it runs the local checks.

/// A machine `dispatchdev` runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Machine {
    /// dispatch-dev, cachyos and CI's runners: the local checks run here.
    Linux,
    /// The owner's MacBook: GitHub runs every check, on the change's PR.
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
    /// Whether the local checks and tests run here. They build, lint and test the Linux hosts'
    /// own code, which macOS fails: the host manager swaps files with `renameat2`, the ops
    /// tests expect a temporary folder that isn't macOS's symlinked `/private/tmp`, and the
    /// server multiplies `statvfs` fields that macOS types otherwise.
    pub const fn checks(self) -> bool {
        matches!(self, Self::Linux)
    }
}
