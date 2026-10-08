use super::*;

#[test]
fn prompt_arguments_are_bounded_and_named() {
    assert!(prompt_text("daily_summary", &Map::new()).unwrap().is_some());
    assert!(
        prompt_text(
            "daily_summary",
            &Map::from_iter([("other".into(), json!("x"))])
        )
        .is_err()
    );
    assert!(
        prompt_text(
            "driver_review",
            &Map::from_iter([("driver".into(), json!("x".repeat(201)))])
        )
        .is_err()
    );
    assert!(prompt_text("missing", &Map::new()).unwrap().is_none());
}

// Core names no feature to agents: with none installed, it says only what it holds itself.
#[test]
fn with_no_feature_agents_are_told_of_no_feature() {
    crate::testing::install(&[], &[]);
    let text = instructions();
    assert!(text.contains("driver also accepts Paycom codes and Amazon transporter IDs."));
    assert!(!text.contains("Driver Match"));
    let skill = crate::mcp::skill::skill("https://dispatch.example.com");
    assert!(skill.contains(
        "description: Answers questions about a delivery service partner's drivers from \
         Dispatch, for one driver or the whole team, on any day or period. Use when asked how \
         a driver did or who led or trailed on a number.\n"
    ));
    let prompts = prompts();
    let review = prompts.iter().find(|p| p.name == "driver_review").unwrap();
    let driver = &review.arguments.as_ref().unwrap()[0];
    assert_eq!(
        driver.description.as_deref(),
        Some("A name, Paycom code or transporter ID.")
    );
}
