use super::*;
use std::time::Duration;

const HEAD: &str = "abc";
fn pull(head: &str, required: Option<Check>) -> Pull {
    Pull {
        number: 7,
        url: "https://github.com/o/r/pull/7".to_owned(),
        title: String::new(),
        body: String::new(),
        head: head.to_owned(),
        required,
    }
}
/// A platform change, committed and pushed, its PR's review done unless `why` says otherwise.
struct Case<'a> {
    dirty: bool,
    pushed: bool,
    checked: bool,
    approval: Approval,
    shown: bool,
    pull: Option<Pull>,
    pending: Option<Pending>,
    progress: Progress,
    why: Option<&'a str>,
}
impl Default for Case<'_> {
    fn default() -> Self {
        Self {
            dirty: false,
            pushed: true,
            checked: true,
            approval: Approval::None,
            shown: false,
            pull: None,
            pending: None,
            progress: Progress::NotStarted,
            why: None,
        }
    }
}
impl Case<'_> {
    fn next(&self) -> (Vec<String>, Whose) {
        let standing = Standing {
            branch: "change",
            head: HEAD,
            dirty: self.dirty,
            pushed: self.pushed,
            checked: self.checked,
            approval: &self.approval,
            shown: self.shown,
            pull: self.pull.as_ref(),
            pending: self.pending.as_ref(),
            progress: &self.progress,
        };
        standing.next(|_| Ok(self.why.map(str::to_owned))).unwrap()
    }
    fn whose(&self) -> Whose {
        self.next().1
    }
}

#[test]
fn work_still_in_the_worktree_or_unpushed_is_the_agents() {
    let dirty = Case {
        dirty: true,
        pull: Some(pull(HEAD, Some(Check::Passed))),
        ..Case::default()
    };
    assert_eq!(
        dirty.next(),
        (
            vec!["Commit it, then `dispatchdev check`".to_owned()],
            Whose::Agent
        )
    );
    let unpushed = Case {
        pushed: false,
        pull: Some(pull("old", Some(Check::Passed))),
        ..Case::default()
    };
    assert_eq!(unpushed.whose(), Whose::Agent);
    let docs = Case {
        checked: false,
        ..Case::default()
    };
    assert_eq!(
        docs.next(),
        (vec!["`dispatchdev pr`".to_owned()], Whose::Agent),
        "nothing checks the agent docs"
    );
}
#[test]
fn a_change_to_what_a_page_shows_waits_on_the_user_once_shown_until_approved() {
    for approval in [
        Approval::Missing,
        Approval::Changed(vec!["a.tsx".to_owned()]),
    ] {
        let unshown = Case {
            approval: approval.clone(),
            ..Case::default()
        };
        assert_eq!(unshown.whose(), Whose::Agent, "showing it is the agent's");
        let shown = Case {
            approval,
            shown: true,
            ..Case::default()
        };
        assert_eq!(shown.whose(), Whose::Owner);
    }
    for approval in [
        Approval::Approved,
        Approval::Unchanged("a refactor".to_owned()),
    ] {
        let approved = Case {
            approval,
            ..Case::default()
        };
        assert_eq!(
            approved.next(),
            (
                vec!["`dispatchdev check`, then `dispatchdev pr`".to_owned()],
                Whose::Agent
            )
        );
    }
}
#[test]
fn an_open_pr_waits_on_github_until_its_checks_end() {
    let checks = |required, pending| Case {
        pull: Some(pull(HEAD, required)),
        pending,
        progress: Progress::Recorded {
            pass: 1,
            done: true,
            head: HEAD.to_owned(),
        },
        ..Case::default()
    };
    let running = Some(Pending::Running(Duration::from_secs(60)));
    assert_eq!(checks(Some(Check::Running), running).whose(), Whose::Github);
    assert_eq!(checks(Some(Check::Missing), None).whose(), Whose::Github);
    assert_eq!(checks(Some(Check::Passed), None).whose(), Whose::Ready);
    assert_eq!(checks(None, None).whose(), Whose::Ready, "the agent docs");
    assert_eq!(
        checks(Some(Check::Failed), None).next(),
        (
            vec!["`dispatchdev ci` says why its checks failed".to_owned()],
            Whose::Agent
        )
    );
    // A run GitHub dropped never starts: starting it again is the agent's.
    let dropped = Some(Pending::Dropped {
        run: 5,
        waited: Duration::from_secs(900),
    });
    assert_eq!(
        checks(Some(Check::Missing), dropped).next(),
        (
            vec![
                ci::restart("change"),
                "`dispatchdev merge`, once its checks passed".to_owned()
            ],
            Whose::Agent
        )
    );
    // GitHub hasn't caught up with the push.
    let behind = Case {
        pull: Some(pull("old", None)),
        ..Case::default()
    };
    assert_eq!(behind.whose(), Whose::Github);
}
#[test]
fn a_review_pass_waits_on_its_reviewers_only_while_they_have_it() {
    let review = |progress, required| Case {
        pull: Some(pull(HEAD, required)),
        progress,
        why: Some("the review's next step"),
        ..Case::default()
    };
    let running = || Some(Check::Running);
    assert_eq!(
        review(Progress::Prepared(1), running()).next(),
        (vec!["the review's next step".to_owned()], Whose::Reviewers)
    );
    assert_eq!(
        review(Progress::Answering(1), running()).whose(),
        Whose::Agent
    );
    let again = Progress::Recorded {
        pass: 1,
        done: false,
        head: HEAD.to_owned(),
    };
    assert_eq!(review(again, running()).whose(), Whose::Agent);
    assert_eq!(
        review(Progress::NotStarted, running()).whose(),
        Whose::Agent
    );
    // Checks that failed while the reviewers read it are the agent's to fix meanwhile.
    assert_eq!(
        review(Progress::Prepared(1), Some(Check::Failed)).whose(),
        Whose::Agent
    );
}
