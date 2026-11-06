//! Checked command output and ownership of assigned subprocess groups.
use crate::Result;
use std::path::Path;
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

pub async fn command(directory: &Path, program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(directory)
        .kill_on_drop(true)
        .output()
        .await?;
    if !output.status.success() {
        // Build and GitHub errors go into the private agent context, never a public event log.
        return Err(format!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
