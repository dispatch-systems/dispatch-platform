use super::ticket;

#[test]
fn a_worktree_is_named_only_by_an_id_its_cleanup_finds_again() {
    let own = "0123456789abcdef0123456789abcdef";
    assert_eq!(ticket(&format!("support_{own}")), Some(own));
    // One short of what the platform gives, one past it, none at all, another prefix, and
    // characters no id holds.
    for id in [
        format!("support_{}", &own[1..]),
        format!("support_{own}0"),
        "support_".to_owned(),
        format!("ticket_{own}"),
        format!("support_{}", own.replace('0', "g")),
    ] {
        assert_eq!(ticket(&id), None, "{id}");
    }
}
