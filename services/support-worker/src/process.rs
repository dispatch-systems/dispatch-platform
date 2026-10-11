//! Cancellation kills the entire assigned subprocess group before authority is relinquished.
use crate::Result;
use tokio::process::{Child, Command};

pub struct Group(pub u32);
impl Drop for Group {
    fn drop(&mut self) {
        // SAFETY: spawn() creates this process group with the child's PID. It is never the
        // worker's own group, and stays owned for precisely this subprocess lifetime.
        unsafe {
            libc::kill(-(self.0 as i32), libc::SIGKILL);
        }
    }
}

pub fn spawn(command: &mut Command) -> Result<(Child, Group)> {
    use std::os::unix::process::CommandExt;
    command.as_std_mut().process_group(0);
    let child = command.kill_on_drop(true).spawn()?;
    let group = Group(child.id().ok_or("Child PID unavailable")?);
    Ok((child, group))
}
