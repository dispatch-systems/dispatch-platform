use super::*;
#[test]
fn tracks_data_dependencies_separately_from_images_and_other_tabs() -> Result<()> {
    let mut loading = Loading::default();
    for session in ["one", "two"] {
        loading.observe(&json!({"method":"Page.frameNavigated","sessionId":session,"params":{"frame":{"loaderId":"new"}}}))?;
    }
    for (id, kind) in [("data", "XHR"), ("script", "Script"), ("image", "Image")] {
        loading.observe(
            &json!({"method":"Network.requestWillBeSent","sessionId":"one",
            "params":{"requestId":id,"loaderId":"new","type":kind}}),
        )?;
    }
    assert_eq!(loading.status("one", "new")["pending"], 2);
    assert_eq!(loading.status("two", "new")["pending"], 0);
    assert_eq!(loading.status("one", "old")["known"], false);
    loading.observe(&json!({"method":"Network.loadingFinished","sessionId":"one","params":{"requestId":"data"}}))?;
    loading.observe(&json!({"method":"Network.loadingFailed","sessionId":"one","params":{"requestId":"script"}}))?;
    assert_eq!(loading.status("one", "new")["pending"], 0);
    assert_eq!(loading.status("one", "new")["failed"], true);
    loading.observe(&json!({"method":"Page.frameNavigated","sessionId":"one","params":{"frame":{"loaderId":"next"}}}))?;
    assert_eq!(loading.status("one", "next")["failed"], false);
    loading.remove("one");
    assert_eq!(loading.status("one", "next")["known"], false);
    Ok(())
}
