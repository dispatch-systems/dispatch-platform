use super::*;

#[test]
fn redirects_keep_their_query_and_any_scheme() {
    assert_eq!(
        redirect(
            "http://127.0.0.1:5/cb",
            &[
                ("code", Some("a b")),
                ("state", None),
                ("iss", Some("https://d.example"))
            ]
        ),
        "http://127.0.0.1:5/cb?code=a+b&iss=https%3A%2F%2Fd.example"
    );
    assert_eq!(
        redirect(
            "https://a.example/cb?x=1",
            &[("error", Some("access_denied"))]
        ),
        "https://a.example/cb?x=1&error=access_denied"
    );
    assert_eq!(
        redirect(
            "cursor://anysphere.cursor-retrieval/oauth/callback",
            &[("code", Some("c")), ("state", Some("s/1"))]
        ),
        "cursor://anysphere.cursor-retrieval/oauth/callback?code=c&state=s%2F1"
    );
}

#[test]
fn pkce_takes_only_the_s256_of_a_proper_verifier() {
    // RFC 7636 Appendix B.
    let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    let challenge = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
    assert!(verifies(verifier, challenge));
    assert!(!verifies(challenge, challenge));
    assert!(!verifies("short", &crypto::s256("short")));
    let spaced = format!("{} ", "a".repeat(43));
    assert!(!verifies(&spaced, &crypto::s256(&spaced)));
}

#[test]
fn repeated_parameters_are_refused_and_empty_ones_are_absent() {
    let query = Query::parse(b"a=1&a=2&b=&c=3&r=x&r=x");
    assert_eq!(query.one("a").unwrap_err().error, "invalid_request");
    assert_eq!(query.one("b").unwrap(), None);
    assert_eq!(query.one("c").unwrap(), Some("3"));
    assert_eq!(query.all("r"), ["x", "x"]);
    assert_eq!(
        query.required("b").unwrap_err().description,
        "b is required"
    );
}
