use super::browser;
use dispatch_core::support::api::types::SupportBrowserGrant;
use serde_json::json;

#[test]
fn browser_grants_require_cookie_security_fields_and_preserve_storage_state() {
    let mut wire = json!({
        "origin":"https://reporter.dispatch.test:8443", "ticketId":"fixture-ticket",
        "expiresAt":90000, "sourceSha":null, "version":null, "release":"fixture",
        "cookie":{
            "name":"__Host-dispatch_support_read", "value":"fixture-cookie",
            "url":"https://reporter.dispatch.test:8443", "path":"/legacy",
            "httpOnly":true, "secure":true, "sameSite":"Strict",
            "domain":"legacy.dispatch.test", "expires":123,
            "partitionKey":"https://dispatch.test"
        }
    });
    let expected = json!({
        "url":"https://reporter.dispatch.test:8443", "readOnly":true,
        "storageState":{
            "cookies":[{
                "name":"__Host-dispatch_support_read", "value":"fixture-cookie",
                "domain":"reporter.dispatch.test", "path":"/", "expires":-1,
                "httpOnly":true, "secure":true, "sameSite":"Strict",
                "partitionKey":"https://dispatch.test"
            }],
            "origins":[]
        }
    });
    let grant: SupportBrowserGrant = serde_json::from_value(wire.clone()).unwrap();
    assert!(grant.source_sha.is_none() && grant.version.is_none());
    assert_eq!(browser(&grant).unwrap(), expected);

    // Older runtimes can omit unavailable source identity, just as they can send null.
    wire.as_object_mut().unwrap().remove("sourceSha");
    wire.as_object_mut().unwrap().remove("version");
    let legacy: SupportBrowserGrant = serde_json::from_value(wire.clone()).unwrap();
    assert!(legacy.source_sha.is_none() && legacy.version.is_none());
    assert_eq!(browser(&legacy).unwrap(), expected);

    let cookie = wire["cookie"].as_object_mut().unwrap();
    let http_only = cookie.remove("httpOnly").unwrap();
    cookie.insert("httponly".into(), http_only);
    let error = serde_json::from_value::<SupportBrowserGrant>(wire)
        .err()
        .expect("a renamed httpOnly field must fail before browser state is constructed");
    assert!(error.to_string().contains("missing field `httpOnly`"));
}
