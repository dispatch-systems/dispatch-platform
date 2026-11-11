use super::{Environment, assets, encoding_quality};
#[test]
fn accepts_encodings_and_respects_explicit_opt_outs() {
    assert_eq!(encoding_quality("gzip, br", "br"), 1.0);
    assert_eq!(encoding_quality("gzip;q=0.5, br;q=0.8", "br"), 0.8);
    assert_eq!(encoding_quality("*;q=1, br;q=0", "br"), 0.0);
    assert_eq!(encoding_quality("*;q=0.5", "gzip"), 0.5);
    assert_eq!(encoding_quality("gzip;q=invalid", "gzip"), 0.0);
    assert_eq!(encoding_quality("", "gzip"), 0.0);
}
#[test]
fn production_shows_the_blue_tab_icon_and_every_other_environment_the_yellow() {
    // The dashboard's own page and icons, as a build copies them.
    let frontend = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|dir| dir.join("Cargo.lock").is_file())
        .expect("repository root")
        .join("app/frontend");
    let root = tempfile::tempdir().unwrap();
    std::fs::copy(frontend.join("index.html"), root.path().join("index.html")).unwrap();
    std::fs::create_dir(root.path().join("assets")).unwrap();
    for (environment, icon, other) in [
        (Environment::Production, "favicon", "favicon-dev"),
        (Environment::Preview, "favicon-dev", "favicon"),
    ] {
        let served = assets(root.path(), "release", environment).unwrap();
        let page = String::from_utf8_lossy(&served["/"].identity.bytes).into_owned();
        for extension in ["svg", "png"] {
            let path = format!("/assets/{icon}.{extension}");
            assert!(
                page.contains(&format!("href=\"{path}\"")),
                "{path} in {page}"
            );
            assert!(!page.contains(&format!("/assets/{other}.{extension}")));
            assert!(frontend.join("public").join(&path[1..]).is_file(), "{path}");
        }
    }
}
