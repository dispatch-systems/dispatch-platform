//! What a member writes, kept as it reads, and what someone at the DSP writes, sent as written.
use super::*;

#[test]
fn slacks_markup_is_kept_as_it_reads() {
    let named = |user: &str| (user == "U0FIXJORDAN").then(|| "Jordan Ellis".to_owned());
    let read = |text: &str| plain(text, named);
    // Escapes, and a mention by the name of whoever it names, or as Slack labelled it.
    assert_eq!(read("Fuel &amp; tolls &lt;3 &gt;"), "Fuel & tolls <3 >");
    assert_eq!(read("ask <@U0FIXJORDAN>"), "ask @Jordan Ellis");
    assert_eq!(read("ask <@U0NOBODY>"), "ask @U0NOBODY");
    assert_eq!(read("<#C0FIXGENERAL|general> and <!here>"), "#general and @here");
    // A link as written, an address when its words are another's.
    assert_eq!(read("see <https://example.com|example.com>"), "see example.com");
    assert_eq!(read("<mailto:a@example.com|a@example.com>"), "a@example.com");
    assert_eq!(
        read("<https://example.com/route|Route sheet>"),
        "Route sheet (https://example.com/route)"
    );
    assert_eq!(read("<https://example.com/a?b=1&amp;c=2>"), "https://example.com/a?b=1&c=2");
    // An unclosed bracket is as written.
    assert_eq!(read("1 &lt; 2 <oops"), "1 < 2 <oops");
}

#[test]
fn what_someone_at_the_dsp_writes_goes_out_as_written() {
    assert_eq!(escaped("<@U1> & <#C1>"), "&lt;@U1&gt; &amp; &lt;#C1&gt;");
    assert_eq!(plain(&escaped("Tolls & fuel <3"), |_| None), "Tolls & fuel <3");
}
