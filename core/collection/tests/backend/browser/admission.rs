use super::*;
#[test]
fn simultaneous_starts_reserve_growth_and_resume_when_memory_returns() {
    let free = 2 * BROWSER_BYTES;
    assert!(Admission::new(Some(free), [].into_iter()).can_start);
    assert!(!Admission::new(Some(free), [0].into_iter()).can_start);
    assert!(Admission::new(Some(free), [BROWSER_BYTES].into_iter()).can_start);
    assert!(!Admission::new(None, [].into_iter()).can_start);
    assert!(!Admission::new(Some(HEADROOM_BYTES), [].into_iter()).can_start);
}
#[test]
fn cgroup_ancestor_limits_and_current_usage_constrain_host_memory() {
    let temp = tempfile::tempdir().unwrap();
    let child = temp.path().join("parent/child");
    fs::create_dir_all(&child).unwrap();
    fs::write(child.join("memory.max"), "max").unwrap();
    fs::write(temp.path().join("parent/memory.max"), "1000").unwrap();
    fs::write(temp.path().join("parent/memory.current"), "600").unwrap();
    assert_eq!(constrained(5000, temp.path(), "/parent/child"), Some(400));
    assert_eq!(constrained(300, temp.path(), "/parent/child"), Some(300));
    fs::write(temp.path().join("parent/memory.current"), "1100").unwrap();
    assert_eq!(constrained(5000, temp.path(), "/parent/child"), Some(0));
    assert_eq!(constrained(5000, temp.path(), "/../escape"), None);
    fs::write(temp.path().join("parent/memory.max"), "invalid").unwrap();
    assert_eq!(constrained(5000, temp.path(), "/parent/child"), None);
}
