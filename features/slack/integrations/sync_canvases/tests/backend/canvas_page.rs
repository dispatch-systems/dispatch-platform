//! A canvas made a page, from Slack's HTML of one: what a document shows of it, in order, with
//! its pictures in it and its mentions named, and nothing a document can't open or show.
use super::*;

/// A canvas as Slack sends it: a title, a line typed over several, two columns holding a
/// picture, a checklist, a link, a mention and a script.
const CANVAS: &str = concat!(
    r#"<div class="quip-canvas-content"><h1 id="temp:C:TEG1">Write-ups</h1>"#,
    r#"<p id="temp:C:TEG2" class="line">Verbal warnings<br><br>Week 41</p>"#,
    r#"<p id="temp:C:TEG3" class="line"></p>"#,
    r#"<div class='flexbox'><div class='flexbox-column'><div data-section-style="11" class=''>"#,
    r#"<img src='/collab-slack-blob/TEG9AAxX4rG/F0FIXPHOTO1?size_name=thumb_1024' id='temp:C:TEG4' "#,
    r#"alt="_SLACK_FILE_ALT_PLACEHOLDER_F0FIXPHOTO1" width='64' height='50'>​</img></div></div>"#,
    r#"<div class='flexbox-column'><p class="line">Dagoberto &amp; Ben</p></div></div>"#,
    r#"<ul class="checklist"><li class="checked">Signed</li><li>Filed</li></ul>"#,
    r#"<p class="line">See <a href="https://example.com/policy">the policy</a>, "#,
    r#"<a>@U0FIXOWNER</a> and <a href="javascript:alert(1)">this</a></p>"#,
    r#"<script>alert(1)</script></div>"#,
);

fn photo() -> Picture {
    Picture {
        mime: "image/png".into(),
        bytes: b"png".to_vec(),
        width: Some(1440),
        height: Some(1120),
    }
}

#[test]
fn a_canvas_names_its_pictures_and_mentions_once_each() {
    assert_eq!(pictures(CANVAS), ["F0FIXPHOTO1"]);
    assert_eq!(mentioned(CANVAS), ["U0FIXOWNER"]);
}

#[test]
fn a_canvas_is_a_page_of_what_a_document_shows_in_order_with_its_pictures_in_it() {
    let pictures = BTreeMap::from([("F0FIXPHOTO1".to_owned(), photo())]);
    let names = BTreeMap::from([("U0FIXOWNER".to_owned(), "Morgan Lane".to_owned())]);
    let made = page(CANVAS, &pictures, &names);
    assert_eq!(
        made,
        concat!(
            r#"<!DOCTYPE html><html><head><meta charset="utf-8"></head><body>"#,
            "<h1>Write-ups</h1>",
            "<p>Verbal warnings<br><br>Week 41</p>",
            "<p></p>",
            // Its columns stack, the picture no wider than a page; its bytes are in it.
            r#"<p><img src="data:image/png;base64,cG5n" width="624" height="485"></p>"#,
            "<p>Dagoberto &amp; Ben</p>",
            "<ul><li>☑ Signed</li><li>☐ Filed</li></ul>",
            r#"<p>See <a href="https://example.com/policy">the policy</a>, @Morgan Lane and this</p>"#,
            "</body></html>",
        )
    );
}

#[test]
fn a_picture_or_member_it_has_no_more_of_is_left_out_or_named_by_their_id() {
    let made = page(CANVAS, &BTreeMap::new(), &BTreeMap::new());
    assert!(!made.contains("<img"));
    assert!(!made.contains("collab-slack-blob"));
    assert!(made.contains("@U0FIXOWNER and this"));
}
