//! The machine `dispatchdev` runs on, by the toolchain it links with.

/// A machine `dispatchdev` runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Machine {
    /// dispatch-dev, cachyos and CI's runners.
    Linux,
    /// The owner's MacBook, whose linker is Apple's.
    Mac,
}
impl Machine {
    /// This machine, by the OS it was built for.
    pub const THIS: Self = if cfg!(target_os = "linux") {
        Self::Linux
    } else {
        Self::Mac
    };
}
