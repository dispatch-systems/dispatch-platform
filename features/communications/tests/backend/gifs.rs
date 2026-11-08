//! A GIF chosen for a message is KLIPY's: only its addresses are taken, a few sizes of one GIF.
//! Slack is given, and a link names, its largest size of 2 MB at most.
use super::*;

fn version(url: &str, size: u64) -> GifVersion {
    GifVersion {
        url: url.into(),
        width: 220,
        height: 124,
        size,
    }
}
fn gif(versions: Vec<GifVersion>) -> MessageGif {
    MessageGif {
        slug: "thumbs-up".into(),
        title: "Thumbs up".into(),
        versions,
    }
}

#[test]
fn a_gif_is_taken_only_from_klipys_addresses() {
    crate::backend::support::install();
    let klipy = gif(vec![
        version("https://static.klipy.com/ii/a/hd.gif", 2_600_000),
        version("https://static2.klipy.com/ii/a/sm.gif", 206_000),
    ]);
    assert_eq!(checked(klipy.clone(), false).unwrap(), klipy);
    for url in [
        "https://example.com/a.gif",
        "http://static.klipy.com/ii/a.gif",
        "https://static.klipy.com.example.com/a.gif",
        "https://static.klipy.com",
        "/api/dsp/communications/gifs/fixture/media/a.gif",
    ] {
        assert!(
            checked(gif(vec![version(url, 1_000)]), false).is_err(),
            "{url}"
        );
    }
    // Fixture mode's stand-in's alone, there.
    let fixture = gif(vec![version(
        "/api/dsp/communications/gifs/fixture/media/thumbs-up-sm.gif",
        206_000,
    )]);
    assert!(checked(fixture.clone(), true).is_ok());
    assert!(checked(klipy, true).is_err());
    // No size, too many, or one of no bytes.
    let one = version("https://static.klipy.com/ii/a/sm.gif", 1_000);
    assert!(checked(gif(Vec::new()), false).is_err());
    assert!(checked(gif(vec![one.clone(); 5]), false).is_err());
    assert!(checked(gif(vec![GifVersion { size: 0, ..one }]), false).is_err());
}

#[test]
fn slack_and_a_link_get_the_largest_size_of_two_megabytes_at_most() {
    let all = gif(vec![
        version("https://static.klipy.com/xs.gif", 64_000),
        version("https://static.klipy.com/hd.gif", 2_600_000),
        version("https://static.klipy.com/md.gif", 1_400_000),
    ]);
    assert_eq!(shared(&all).url, "https://static.klipy.com/md.gif");
    let large = gif(vec![
        version("https://static.klipy.com/hd.gif", 4_000_000),
        version("https://static.klipy.com/md.gif", 3_000_000),
    ]);
    assert_eq!(shared(&large).url, "https://static.klipy.com/md.gif");
    assert_eq!(linked("", &all), "https://static.klipy.com/md.gif");
    assert_eq!(
        linked("Nice work", &all),
        "Nice work\nhttps://static.klipy.com/md.gif"
    );
}
