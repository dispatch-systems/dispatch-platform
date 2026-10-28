//! Requests against the real router, served in-process over a loopback socket.
//! The expectations were recorded before the route table existed, so they pin
//! how the HTTP layer answers rather than how it is built.
// A build that leaves features out drops the tests that need them, and what only they use.
#![cfg_attr(not(feature = "default"), allow(dead_code, unused_imports))]
use crate::support::{Call, SCRIPT, Server, Who};
use dispatch_core::{
    db::{self, s},
    foundation::crypto,
    server::live::Topic,
};
use dispatch_paycom as paycom;
use serde_json::{Value, json};
use std::sync::atomic::Ordering;

#[tokio::test]
async fn ordinary_members_cannot_distinguish_absent_and_foreign_dsps() {
    let server = Server::start().await;
    let mut member = server.member("member@dispatch.test").await;
    let foreign = server.dsp("Summit Delivery").await;
    let absent = format!("dsp_{}", "0".repeat(32));

    let foreign_open = server
        .send(Call::post("/api/session/dsp", json!({"dspId":foreign.clone()})).who(&member))
        .await;
    let absent_open = server
        .send(Call::post("/api/session/dsp", json!({"dspId":absent.clone()})).who(&member))
        .await;
    assert_eq!(
        (foreign_open.status, &foreign_open.body),
        (absent_open.status, &absent_open.body)
    );
    assert_eq!(
        (foreign_open.status, foreign_open.error()),
        (403, "permission_denied")
    );

    member.view = Some(format!("{foreign}.invalid"));
    let foreign_view = server
        .send(Call::get("/api/dsp/employees").who(&member))
        .await;
    member.view = Some(format!("{absent}.invalid"));
    let absent_view = server
        .send(Call::get("/api/dsp/employees").who(&member))
        .await;
    assert_eq!(
        (foreign_view.status, &foreign_view.body),
        (absent_view.status, &absent_view.body)
    );
    assert_eq!(
        (foreign_view.status, foreign_view.error()),
        (403, "permission_denied")
    );

    let owner = server.session("owner@dispatch.test").await;
    server
        .expect(
            Call::post("/api/session/dsp", json!({"dspId":absent.clone()})).who(&owner),
            404,
            "dsp_not_found",
        )
        .await;
    server
        .expect(
            Call::post(
                "/api/session/dsp",
                json!({"dspId":absent.clone(),"roleId":format!("role_{}", "0".repeat(32))}),
            )
            .who(&owner),
            404,
            "dsp_not_found",
        )
        .await;
    let mut owner_preview = owner;
    owner_preview.view = Some(format!("{absent}.role_{}.invalid", "0".repeat(32)));
    server
        .expect(
            Call::get("/api/dsp/employees").who(&owner_preview),
            404,
            "dsp_not_found",
        )
        .await;
}

#[tokio::test]
async fn revoked_connection_manager_cannot_persist_a_pending_provider_result() {
    let server = Server::start().await;
    let dsp = server.dsp("Northline Logistics").await;
    let setup_dsp = dsp.clone();
    server
        .state
        .run(move |db| {
            db.dsp(&setup_dsp)?.exec(
                "UPDATE roles SET permissions='[\"connections.manage\"]' WHERE id=(SELECT \
                 m.role_id FROM memberships m JOIN users u ON u.id=m.user_id WHERE \
                 u.email='member@dispatch.test' AND m.dsp_id=?)",
                [&setup_dsp],
            )?;
            db.platform
                .exec("UPDATE dsps SET revision=revision+1 WHERE id=?", [&setup_dsp])?;
            let credentials = json!({"clientCode":"DEMO1","username":"fixture-user",
                "password":"slow-valid-password","securityAnswers":["one","two","three","four","five"]});
            let area = db.area(&setup_dsp, "secrets")?;
            let key = db::key_file(&area.join("vault.key"))?;
            db::write_private(
                &area.join("paycom.enc"),
                crypto::encrypt(&key, &format!("{setup_dsp}:paycom:2"), &credentials)?
                    .as_bytes(),
            )?;
            db.collector(&setup_dsp, paycom::PROVIDER)?.exec(
                "UPDATE connections SET status='not_connected',error=NULL,verified_at=NULL",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    let member = server.member("member@dispatch.test").await;

    let revoke_state = server.state.clone();
    let revoke_dsp = dsp.clone();
    let request =
        server.send(Call::post("/api/dsp/connections/paycom/check", json!({})).who(&member));
    let revoke = async move {
        let mut pending = false;
        for _ in 0..200 {
            let id = revoke_dsp.clone();
            let status = revoke_state
                .read(move |db| {
                    Ok(db
                        .collector(&id, paycom::PROVIDER)?
                        .one("SELECT status FROM connections WHERE provider='paycom'", [])?
                        .and_then(|row| row["status"].as_str().map(str::to_owned))
                        .unwrap_or_default())
                })
                .await
                .unwrap();
            if status == "signing_in" {
                pending = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        assert!(pending, "provider request never reached its pending state");
        revoke_state
            .run(move |db| {
                db.dsp(&revoke_dsp)?.exec(
                    "UPDATE roles SET permissions='[]' WHERE id=(SELECT m.role_id FROM \
                     memberships m JOIN users u ON u.id=m.user_id WHERE \
                     u.email='member@dispatch.test' AND m.dsp_id=?)",
                    [&revoke_dsp],
                )?;
                db.platform.exec(
                    "UPDATE dsps SET revision=revision+1 WHERE id=?",
                    [&revoke_dsp],
                )?;
                Ok(())
            })
            .await
            .unwrap();
    };
    let (answer, ()) = tokio::join!(request, revoke);
    assert_eq!((answer.status, answer.error()), (403, "permission_denied"));
    assert_eq!(server.state.browsers.active(), 0);
    let final_dsp = dsp.clone();
    let status = server
        .state
        .read(move |db| {
            Ok(db
                .collector(&final_dsp, paycom::PROVIDER)?
                .one("SELECT status FROM connections WHERE provider='paycom'", [])?
                .unwrap()["status"]
                .as_str()
                .unwrap()
                .to_owned())
        })
        .await
        .unwrap();
    assert_eq!(
        status, "signing_in",
        "revoked provider result was persisted"
    );
}

/// A live reader never takes over a browser someone else opened for the DSP: a member's, idle
/// once they signed in, or waiting for them to verify. The read is refused (`connection_busy`),
/// and the member's browser stays theirs as it was, still verifying.
#[cfg(feature = "live_route_data")]
#[tokio::test]
async fn a_live_reader_never_takes_over_a_browser_a_member_opened() {
    use chrono::Timelike;
    use dispatch_core::{collection::readers, testing};
    use dispatch_cortex as cortex;
    for password in ["fixture-password", "require-verification"] {
        let server = Server::start().await;
        let dsp = server.dsp("Northline Logistics").await;
        // Northline at a station where it's late morning, inside Live Route Data's window since
        // eight, and the feature switched on.
        let hour = chrono::Utc::now().hour() as i32;
        let zone = format!("Etc/GMT{:+}", hour - 11);
        let setup_dsp = dsp.clone();
        server
            .state
            .run(move |db| {
                testing::set_dsp(db, &setup_dsp, "Northline Logistics", &zone)?;
                db.set_profile(
                    &setup_dsp,
                    json!({"stationCode":"TST1","abbreviation":"NLL"}),
                )?;
                let owner = testing::platform_owner(db);
                db.set_feature(&setup_dsp, "live_route_data", true, &owner)
                    .map(drop)
            })
            .await
            .unwrap();
        let owner = server.member("owner@dispatch.test").await;
        let credentials = json!({"username":"fixture@example.test","password":password});
        let save = Call::post("/api/dsp/connections/cortex/save", credentials).who(&owner);
        let saved = server.send(save).await;
        assert_eq!(saved.status, 200, "{}", saved.body);
        let opened = server
            .state
            .browsers
            .get_for(&dsp, cortex::PROVIDER)
            .unwrap();
        let verifying = password == "require-verification";
        assert_eq!(
            (opened.ready(), opened.challenge()),
            (!verifying, verifying)
        );

        // Its window open for hours, the reader reads at once.
        let (stop, stopping) = tokio::sync::watch::channel(false);
        let reading = tokio::spawn(readers::start(server.state.clone(), stopping));
        let started = std::time::Instant::now();
        let read = loop {
            let at = dsp.clone();
            let reads = server
                .state
                .read(move |db| testing::live_reads(db, &at))
                .await
                .unwrap();
            if let Some(read) = reads.into_iter().next() {
                break (read.outcome, read.code);
            }
            assert!(started.elapsed().as_secs() < 10, "the reader never read");
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        };
        stop.send_replace(true);
        reading.await.unwrap().unwrap();
        assert_eq!(read, ("failed".into(), Some("connection_busy".into())));
        let kept = server
            .state
            .browsers
            .get_for(&dsp, cortex::PROVIDER)
            .unwrap();
        assert_eq!(kept.id, opened.id);
        assert_eq!(
            (kept.closed(), kept.ready(), kept.challenge()),
            (false, !verifying, verifying)
        );
        if verifying {
            let code = json!({"code":"123456"});
            let verify = Call::post("/api/dsp/connections/cortex/verify", code).who(&owner);
            let verified = server.send(verify).await;
            assert_eq!(verified.status, 200, "{}", verified.body);
            assert_eq!(verified.body["status"], "ready");
        }
    }
}

/// A connection's revision, which what a live reader took from a sign-in is bound to: a new one
/// once its credentials are replaced, and none while it's disabled.
#[tokio::test]
async fn a_connections_revision_moves_on_with_its_credentials_and_is_none_while_disabled() {
    use dispatch_cortex as cortex;
    let server = Server::start().await;
    let dsp = server.dsp("Northline Logistics").await;
    let owner = server.member("owner@dispatch.test").await;
    let revision = || {
        let dsp = dsp.clone();
        server
            .state
            .read(move |db| db.connection_revision(&dsp, cortex::PROVIDER))
    };
    let mut revisions = Vec::new();
    for username in ["first@example.test", "second@example.test"] {
        let credentials = json!({"username":username,"password":"fixture-password"});
        let save = Call::post("/api/dsp/connections/cortex/save", credentials).who(&owner);
        assert_eq!(server.send(save).await.status, 200);
        revisions.push(revision().await.unwrap().unwrap());
    }
    assert!(revisions[1] > revisions[0], "{revisions:?}");
    let disable = Call::post("/api/dsp/connections/cortex/disable", json!({})).who(&owner);
    assert_eq!(server.send(disable).await.status, 200);
    assert_eq!(revision().await.unwrap(), None);
}

#[cfg(all(feature = "timecard", feature = "team"))]
#[tokio::test]
async fn unmatched_paths_and_methods_answer_as_they_always_have() {
    let server = Server::start().await;
    let owner = server.session("owner@dispatch.test").await;
    let member = server.member("member@dispatch.test").await;
    // Outside the DSP and platform areas nothing is authenticated before the 404.
    server
        .expect(Call::get("/api/nope"), 404, "not_found")
        .await;
    server.expect(Call::get("/nope"), 404, "not_found").await;
    server
        .expect(Call::get("/api/invitations"), 404, "not_found")
        .await;
    let call = Call::post("/api/invitations/some-token", json!({}));
    server.expect(call, 404, "not_found").await;
    // A known path with the wrong method is unmatched, never 405.
    server
        .expect(Call::post("/api/health", json!({})), 404, "not_found")
        .await;
    server
        .expect(Call::post("/api/session", json!({})), 404, "not_found")
        .await;
    server
        .expect(Call::get("/api/auth/login"), 404, "not_found")
        .await;
    server
        .expect(Call::new("HEAD", "/api/health"), 404, "")
        .await;
    let wrong = server.send(Call::post("/api/session", json!({}))).await;
    assert_eq!((wrong.status, wrong.header("allow")), (404, ""));
    let call = Call::new("PUT", "/api/session").raw("{}".into());
    server.expect(call, 404, "not_found").await;
    let call = Call::new("DELETE", "/api/session").raw("{}".into());
    server
        .expect(call.without_origin(), 403, "invalid_origin")
        .await;
    server
        .expect(Call::new("OPTIONS", "/api/session"), 404, "not_found")
        .await;
    // The DSP and platform areas authenticate before admitting a path is unknown.
    server
        .expect(Call::get("/api/dsp/nope"), 401, "sign_in_required")
        .await;
    server
        .expect(Call::get("/api/platform/nope"), 401, "sign_in_required")
        .await;
    let call = Call::get("/api/platform/nope").who(&member);
    server.expect(call, 403, "platform_owner_required").await;
    server
        .expect(
            Call::get("/api/platform/nope").who(&owner),
            404,
            "not_found",
        )
        .await;
    server
        .expect(
            Call::get("/api/dsp/nope").who(&owner),
            403,
            "dsp_view_required",
        )
        .await;
    // A path in no part of the DSP area asks for what only an owner holds.
    let call = Call::get("/api/dsp/nope").who(&member);
    server.expect(call, 403, "permission_denied").await;
    let viewing = server.member("owner@dispatch.test").await;
    let call = Call::get("/api/dsp/nope").who(&viewing);
    server.expect(call, 404, "not_found").await;
    let call = Call::get("/api/dsp/profile").who(&member);
    server.expect(call, 403, "permission_denied").await;
    // An unknown path still asks for the permission its area asked for.
    let call = Call::post("/api/dsp/employees", json!({})).who(&member);
    server.expect(call, 404, "not_found").await;
    let call = Call::post("/api/dsp/members", json!({})).who(&member);
    server.expect(call, 403, "permission_denied").await;
    let call = Call::get("/api/dsp/connections/paycom/check").who(&member);
    server.expect(call, 403, "permission_denied").await;
    let call = Call::get("/api/dsp/audit/nope").who(&member);
    server.expect(call, 403, "permission_denied").await;
    let call = Call::post("/api/dsp/nope", json!({}))
        .who(&member)
        .without_csrf();
    server.expect(call, 403, "csrf_required").await;
    // Path segments are taken as sent: never decoded, and an empty one still
    // reaches the handler that validates it.
    let call = Call::get("/api/dsp/employees/").who(&member);
    server.expect(call, 400, "invalid_employee_code").await;
    let call = Call::get("/api/dsp/employees/E%30%301").who(&member);
    server.expect(call, 400, "invalid_employee_code").await;
    let call = Call::get("/api/dsp/employees/E001/").who(&member);
    server.expect(call, 404, "not_found").await;
    server
        .expect(Call::get("/api/invitations/"), 404, "invitation_expired")
        .await;
    let call = Call::get("/api/dsp/connections/unknown").who(&member);
    server.expect(call, 403, "permission_denied").await;
}

#[tokio::test]
async fn the_request_pipeline_checks_host_origin_content_type_and_size() {
    let server = Server::start().await;
    let call = Call::get("/api/health").header("host", "evil.example");
    server.expect(call, 400, "invalid_host").await;
    let call = Call::post("/api/auth/login", json!({})).without_origin();
    server.expect(call, 403, "invalid_origin").await;
    let call = Call::post("/api/auth/login", json!({})).header("origin", "http://evil.example");
    server.expect(call, 403, "invalid_origin").await;
    let call = Call::post("/api/auth/login", json!({})).header("content-type", "text/plain");
    server.expect(call, 415, "json_required").await;
    // A file goes only to a route that takes uploads.
    let call =
        Call::post("/api/auth/login", json!({})).header("content-type", "application/octet-stream");
    server.expect(call, 415, "json_required").await;
    let call = Call::new("POST", "/api/auth/login").raw("{".into());
    server.expect(call, 400, "invalid_input").await;
    // Malformed requests are refused before the path is looked at.
    let call = Call::new("POST", "/api/nope").raw("{".into());
    server.expect(call, 400, "invalid_input").await;
    server
        .expect(Call::get("/api/nope?a=1&a=2"), 400, "invalid_input")
        .await;
    let call = Call::new("POST", "/api/nope").raw(format!("\"{}\"", "x".repeat(70 * 1024)));
    server.expect(call, 413, "request_too_large").await;
    // Health answers before the query is parsed.
    let health = server.send(Call::get("/api/health?a=1&a=2")).await;
    assert_eq!(health.status, 200);
    assert_eq!(
        health.body,
        json!({
            "status":"ready",
            "environment":"preview",
            "release":server.state.config.release,
            "source":server.state.config.source,
            "runtime":"rust"
        })
    );
    for answer in [health, server.send(Call::get("/api/nope")).await] {
        assert!(answer.header("x-request-id").starts_with("req_"));
        assert_eq!(answer.header("x-content-type-options"), "nosniff");
        assert_eq!(answer.header("referrer-policy"), "same-origin");
        assert_eq!(answer.header("x-frame-options"), "DENY");
        assert_eq!(answer.header("cross-origin-opener-policy"), "same-origin");
        assert_eq!(answer.header("cross-origin-resource-policy"), "same-origin");
        assert_eq!(
            answer.header("permissions-policy"),
            "accelerometer=(), camera=(), geolocation=(), gyroscope=(), microphone=(), payment=(), usb=(), publickey-credentials-create=(self), publickey-credentials-get=(self)"
        );
        assert_eq!(answer.header("cache-control"), "no-store");
        assert_eq!(
            answer.header("content-security-policy"),
            "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' \
             'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self' blob: ws:; font-src 'self'; \
             object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'"
        );
    }
    let update = server.send(Call::get("/api/browser-update")).await;
    assert_eq!(update.status, 200);
    assert_eq!(update.header("cache-control"), "no-store");
    assert_eq!(
        update.body["build"],
        crypto::sha(server.state.config.release.as_bytes())
    );
}

#[cfg(all(feature = "timecard", feature = "team", feature = "settings"))]
#[tokio::test]
async fn every_access_kind_refuses_and_admits_the_right_callers() {
    let server = Server::start().await;
    let owner = server.session("owner@dispatch.test").await;
    let member = server.member("member@dispatch.test").await;
    // Public.
    server
        .expect(
            Call::get("/api/invitations/unknown"),
            404,
            "invitation_expired",
        )
        .await;
    let call = Call::post(
        "/api/auth/forgot-password",
        json!({"email":"nobody@dispatch.test"}),
    );
    let answer = server.send(call).await;
    assert_eq!((answer.status, &answer.body), (202, &json!({"ok":true})));
    // Session.
    server
        .expect(Call::get("/api/session"), 401, "sign_in_required")
        .await;
    let call = Call::post("/api/auth/logout", json!({}))
        .who(&member)
        .without_csrf();
    server.expect(call, 403, "csrf_required").await;
    let session = server.send(Call::get("/api/session").who(&member)).await;
    assert_eq!(session.status, 200);
    assert_eq!(session.body["user"]["email"], "member@dispatch.test");
    // Platform owner.
    server
        .expect(Call::get("/api/platform/dsps"), 401, "sign_in_required")
        .await;
    let call = Call::get("/api/platform/dsps").who(&member);
    server.expect(call, 403, "platform_owner_required").await;
    let call = Call::post("/api/platform/dsps/x/status", json!({})).who(&member);
    server.expect(call, 403, "platform_owner_required").await;
    let dsps = server
        .send(Call::get("/api/platform/dsps").who(&owner))
        .await;
    assert_eq!(dsps.status, 200);
    assert_eq!(dsps.body.as_array().unwrap().len(), 3);
    // DSP permission.
    server
        .expect(Call::get("/api/dsp/employees"), 401, "sign_in_required")
        .await;
    server
        .expect(
            Call::get("/api/dsp/employees").who(&owner),
            403,
            "dsp_view_required",
        )
        .await;
    for path in [
        "/api/dsp/jobs",
        "/api/dsp/schedules",
        "/api/dsp/connections",
        "/api/dsp/connections/paycom",
        "/api/dsp/members",
        "/api/dsp/invitations",
    ] {
        server
            .expect(Call::get(path).who(&member), 403, "permission_denied")
            .await;
    }
    for (path, body) in [
        ("/api/dsp/jobs", json!({})),
        ("/api/dsp/jobs/meal-breaks", json!({})),
        ("/api/dsp/jobs/job_x/cancel", json!({})),
        ("/api/dsp/profile", json!({})),
        ("/api/dsp/schedules/preview", json!({})),
        ("/api/dsp/roles", json!({})),
        ("/api/dsp/members/invite", json!({})),
        ("/api/dsp/members/mem_x", json!({})),
        ("/api/dsp/connections/paycom/check", json!({})),
        ("/api/dsp/cortex/meal-breaks/collect", json!({})),
    ] {
        server
            .expect(
                Call::post(path, body).who(&member),
                403,
                "permission_denied",
            )
            .await;
    }
    let employees = server
        .send(Call::get("/api/dsp/employees?limit=2").who(&member))
        .await;
    assert_eq!(employees.status, 200, "{}", employees.body);
    assert_eq!(employees.body["employees"].as_array().unwrap().len(), 2);
    let code = s(&employees.body["employees"][0], "code").to_owned();
    let employee = server
        .send(Call::get(&format!("/api/dsp/employees/{code}")).who(&member))
        .await;
    assert_eq!(employee.status, 200, "{}", employee.body);
    let call = Call::post("/api/dsp/presence", json!({"tab":"t1","state":"active"})).who(&member);
    let presence = server.send(call).await;
    assert_eq!(
        (presence.status, &presence.body),
        (200, &json!({"ok":true}))
    );
    // Input is validated before the session on the routes that always did so.
    let call = Call::post("/api/dsp/presence", json!({}));
    server.expect(call, 400, "invalid_input").await;
    server
        .expect(
            Call::get("/api/dsp/collection-updates?x=1"),
            400,
            "invalid_input",
        )
        .await;
    let call = Call::post("/api/auth/password", json!({}));
    server.expect(call, 400, "invalid_input").await;
}

#[cfg(all(feature = "timecard", feature = "settings"))]
#[tokio::test]
async fn only_dsp_and_schedule_writes_that_succeed_wake_the_scheduler() {
    let server = Server::start().await;
    let owner = server.member("owner@dispatch.test").await;
    let revision = || server.state.schedule_revision.load(Ordering::Acquire);
    let before = revision();
    let call = Call::post("/api/dsp/profile", json!({})).who(&owner);
    server.expect(call, 400, "invalid_input").await;
    let call = Call::post("/api/dsp/collectors/paycom/preview", json!({})).who(&owner);
    assert_ne!(server.send(call).await.status, 404);
    assert_eq!(revision(), before);
    let profile = json!({
        "name":"Northline Logistics",
        "timezone":"America/Chicago",
        "abbreviation":"NLL",
        "stationCode":"DCH1"
    });
    let saved = server
        .send(Call::post("/api/dsp/profile", profile).who(&owner))
        .await;
    assert_eq!((saved.status, &saved.body), (200, &json!({"ok":true})));
    assert_eq!(revision(), before + 1);
    let created = server
        .send(Call::post("/api/platform/dsps", json!({"name":"Fourth DSP"})).who(&owner))
        .await;
    assert_eq!(created.status, 201, "{}", created.body);
    assert_eq!(revision(), before + 2);
    let id = s(&created.body["dsp"], "id").to_owned();
    let path = format!("/api/platform/dsps/{id}/retry");
    let call = Call::post(&path, json!({})).who(&owner);
    server.expect(call, 409, "dsp_already_initialized").await;
    assert_eq!(revision(), before + 2);
    // Suspending a DSP has never woken the scheduler.
    let path = format!("/api/platform/dsps/{id}/status");
    let suspended = server
        .send(Call::post(&path, json!({"status":"suspended"})).who(&owner))
        .await;
    assert_eq!(suspended.status, 200, "{}", suspended.body);
    assert_eq!(suspended.body["status"], "suspended");
    assert_eq!(revision(), before + 2);
    // A save in Settings wakes it only when it moves when the DSP's schedules run, as its
    // timezone does: not for a feature's choice, nor the DSP's station.
    let mut woken = revision();
    for (id, values, wakes) in [
        ("timecard.drivers", json!({"all_departments": false}), false),
        ("settings.profile", json!({"station_code": "DCH2"}), false),
        ("settings.profile", json!({"timezone": "Asia/Tokyo"}), true),
    ] {
        // The DSP opened afresh each time, as a save of its profile expires its views.
        let owner = server.member("owner@dispatch.test").await;
        let read = server
            .send(Call::get("/api/dsp/settings").who(&owner))
            .await;
        let sections = read.body["sections"].as_array().unwrap();
        let section = sections.iter().find(|section| section["id"] == id).unwrap();
        let save = json!({"values": values, "revision": section["revision"]});
        let path = format!("/api/dsp/settings/{id}");
        let saved = server.send(Call::post(&path, save).who(&owner)).await;
        assert_eq!(saved.status, 200, "{id}: {}", saved.body);
        woken += u64::from(wakes);
        assert_eq!(revision(), woken, "{id}: {values}");
    }
}

// Settings serves the DSP's profile, the DSP route it writes through: core has none of its own.
#[cfg(feature = "settings")]
#[tokio::test]
async fn a_write_on_a_dsps_route_evicts_only_that_dsps_cached_reads() {
    use dispatch_core::server::cache::Scope;
    let server = Server::start().await;
    let owner = server.member("owner@dispatch.test").await;
    let north = server.dsp("Northline Logistics").await;
    let summit = server.dsp("Summit Delivery").await;
    // A cached read of each DSP's: how many times any was read afresh, once this one is read.
    let loads = std::sync::atomic::AtomicUsize::new(0);
    let read = |dsp: &str| {
        let revision = server.state.data_revision.load(Ordering::Relaxed);
        let scope = Scope::tenant("probe", dsp);
        server
            .state
            .read_cache
            .read(scope, "probe".into(), revision, || {
                loads.fetch_add(1, Ordering::Relaxed);
                Ok(())
            })
            .unwrap();
        loads.load(Ordering::Relaxed)
    };
    assert_eq!((read(&north), read(&summit), read(&north)), (1, 2, 2));
    // Northline's profile, saved on its own route: only its reads are read afresh.
    let profile = json!({
        "name":"Northline Logistics",
        "timezone":"America/Chicago",
        "abbreviation":"NLL",
        "stationCode":"DCH1"
    });
    let saved = server
        .send(Call::post("/api/dsp/profile", profile).who(&owner))
        .await;
    assert_eq!(saved.status, 200, "{}", saved.body);
    assert_eq!((read(&north), read(&summit)), (3, 3));
    // The platform owner's write names no DSP, so every DSP's reads are.
    let created = server
        .send(Call::post("/api/platform/dsps", json!({"name":"Fourth DSP"})).who(&owner))
        .await;
    assert_eq!(created.status, 201, "{}", created.body);
    assert_eq!((read(&north), read(&summit)), (4, 5));
}

#[tokio::test]
async fn signing_in_and_out_sets_and_clears_the_session_cookie() {
    let server = Server::start().await;
    let credentials = json!({"email":"owner@dispatch.test","password":"Dispatch-demo-2026!"});
    let login = server
        .send(Call::post("/api/auth/login", credentials))
        .await;
    assert_eq!((login.status, &login.body), (200, &json!({"ok":true})));
    let cookie = login.header("set-cookie").to_owned();
    let (pair, attributes) = cookie.split_once(';').unwrap();
    assert_eq!(
        attributes,
        " Path=/; HttpOnly; SameSite=Strict; Max-Age=28800"
    );
    let mut who = Who {
        cookie: pair.to_owned(),
        csrf: String::new(),
        view: None,
        host: None,
    };
    let session = server.send(Call::get("/api/session").who(&who)).await;
    who.csrf = s(&session.body, "csrf").to_owned();
    let logout = server
        .send(Call::post("/api/auth/logout", json!({})).who(&who))
        .await;
    assert_eq!((logout.status, &logout.body), (200, &json!({"ok":true})));
    assert_eq!(
        logout.header("set-cookie"),
        "dispatch_session=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0"
    );
    server
        .expect(Call::get("/api/session").who(&who), 401, "sign_in_required")
        .await;
}

#[cfg(all(feature = "uniforms", feature = "timecard"))]
#[tokio::test(flavor = "current_thread")]
async fn open_update_waits_reauthorize_after_the_session_expires() {
    let server = Server::start().await;
    let dsp = server.dsp("Northline Logistics").await;
    for uniforms in [false, true] {
        let who = server.member("owner@dispatch.test").await;
        let endpoint = if uniforms {
            "/api/dsp/uniforms/updates"
        } else {
            "/api/dsp/collection-updates"
        };
        let initial_path = if uniforms {
            endpoint.to_owned()
        } else {
            format!("{endpoint}?after=initial")
        };
        let initial = server.send(Call::get(&initial_path).who(&who)).await;
        assert_eq!(initial.status, 200, "{}", initial.body);
        let after = if uniforms {
            initial.body["revision"].as_i64().unwrap().to_string()
        } else {
            s(&initial.body, "revision").to_owned()
        };
        let path = format!("{endpoint}?after={after}");
        let request = server.send(Call::get(&path).who(&who));
        tokio::pin!(request);
        // Uniforms waits on its own topic; collection progress on core's hub.
        let slots = if uniforms {
            &server.state.topics.slots
        } else {
            &server.state.updates.slots
        };
        let waiter_capacity = slots.available_permits();
        let database_capacity = server.state.db_queue.available_permits();
        // The waiter permit is acquired only after successful authorization. On this
        // current-thread runtime, yielding with an empty DB queue lets the handler
        // finish its initial reads and enter the watch wait before access is revoked.
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                tokio::select! {
                    biased;
                    answer = &mut request => panic!("wait returned before revocation: {}", answer.body),
                    _ = tokio::task::yield_now() => {}
                }
                if slots.available_permits() + 1 == waiter_capacity
                    && server.state.db_queue.available_permits() == database_capacity
                {
                    tokio::select! {
                        biased;
                        answer = &mut request => panic!("wait returned before revocation: {}", answer.body),
                        _ = tokio::task::yield_now() => {}
                    }
                    if server.state.db_queue.available_permits() == database_capacity {
                        break;
                    }
                }
            }
        }).await.expect("the authorized request must acquire a waiter and finish its initial DB reads");
        let hash = crypto::sha(who.cookie.strip_prefix("dispatch_session=").unwrap());
        server
            .state
            .run(move |db| {
                db.platform
                    .exec("UPDATE sessions SET expires_at=0 WHERE hash=?", [hash])?;
                Ok(())
            })
            .await
            .unwrap();
        if uniforms {
            server.state.topics.notify(Topic("uniforms"), &dsp);
        } else {
            server.state.updates.notify(&dsp);
        }
        let answer = tokio::time::timeout(std::time::Duration::from_secs(5), request)
            .await
            .expect("revoking the session and notifying updates must release the waiting request");
        assert_eq!((answer.status, answer.error()), (401, "sign_in_required"));
        assert_eq!(slots.available_permits(), waiter_capacity);
    }
}

#[tokio::test]
async fn remembered_sessions_have_a_fixed_seven_day_deadline() {
    let server = Server::start().await;
    for value in [json!("true"), json!(1), Value::Null] {
        server.expect(Call::post("/api/auth/login", json!({
            "email":"owner@dispatch.test", "password":"Dispatch-demo-2026!", "rememberMe":value
        })), 400, "invalid_input").await;
    }
    for (remember, seconds) in [(false, 28800), (true, 604800)] {
        let login = server.send(Call::post("/api/auth/login", json!({
            "email":"owner@dispatch.test", "password":"Dispatch-demo-2026!", "rememberMe":remember
        }))).await;
        assert_eq!(login.status, 200, "{}", login.body);
        let (pair, attributes) = login.header("set-cookie").split_once(';').unwrap();
        assert_eq!(
            attributes,
            format!(" Path=/; HttpOnly; SameSite=Strict; Max-Age={seconds}")
        );
        let hash = crypto::sha(pair.strip_prefix("dispatch_session=").unwrap());
        let query_hash = hash.clone();
        let (expiry, created): (i64, i64) = server
            .state
            .read(move |db| {
                Ok(db
                    .platform
                    .one_as(
                        "SELECT expires_at,created_at FROM sessions WHERE hash=?",
                        [&query_hash],
                    )?
                    .unwrap())
            })
            .await
            .unwrap();
        assert_eq!(expiry - created, seconds * 1000);
        let who = Who {
            cookie: pair.to_owned(),
            csrf: String::new(),
            view: None,
            host: None,
        };
        for _ in 0..2 {
            let session = server.send(Call::get("/api/session").who(&who)).await;
            assert_eq!(session.status, 200);
            assert!(
                session.header("set-cookie").is_empty(),
                "activity must not renew the cookie"
            );
        }
        let query_hash = hash.clone();
        let (unchanged,): (i64,) = server
            .state
            .read(move |db| {
                Ok(db
                    .platform
                    .one_as(
                        "SELECT expires_at FROM sessions WHERE hash=?",
                        [&query_hash],
                    )?
                    .unwrap())
            })
            .await
            .unwrap();
        assert_eq!(
            unchanged, expiry,
            "activity must not slide the server deadline"
        );
        server
            .state
            .run(move |db| {
                db.platform.exec(
                    "UPDATE sessions SET expires_at=? WHERE hash=?",
                    rusqlite::params![db::now(), hash],
                )?;
                Ok(())
            })
            .await
            .unwrap();
        server
            .expect(Call::get("/api/session").who(&who), 401, "sign_in_required")
            .await;
    }
}

#[tokio::test]
async fn the_dashboard_is_served_with_validators_and_everything_else_is_not_found() {
    let server = Server::start().await;
    let index = server.send(Call::get("/")).await;
    assert_eq!(index.status, 200);
    assert_eq!(index.header("content-type"), "text/html; charset=utf-8");
    assert_eq!(index.header("cache-control"), "no-store");
    let script = server.send(Call::get(SCRIPT)).await;
    assert_eq!(script.status, 200);
    assert_eq!(
        script.header("content-type"),
        "text/javascript; charset=utf-8"
    );
    assert_eq!(
        script.header("cache-control"),
        "public, max-age=31536000, immutable"
    );
    assert_eq!(script.header("content-length"), "14");
    let etag = script.header("etag").to_owned();
    let cached = server
        .send(Call::get(SCRIPT).header("if-none-match", &etag))
        .await;
    assert_eq!(cached.status, 304);
    assert_eq!(cached.header("etag"), etag);
    let head = server.send(Call::new("HEAD", SCRIPT)).await;
    assert_eq!((head.status, head.header("content-length")), (200, "14"));
    // Assets answer before the query is parsed; nothing else does.
    assert_eq!(server.send(Call::get("/?a=1&a=2")).await.status, 200);
    server
        .expect(Call::get("/assets/missing.js"), 404, "not_found")
        .await;
    server.expect(Call::get("/assets/"), 404, "not_found").await;
    server
        .expect(Call::get("/assets/..%2Findex.html"), 404, "not_found")
        .await;
    server
        .expect(Call::post(SCRIPT, json!({})), 404, "not_found")
        .await;
    server
        .expect(Call::get("/index.html"), 404, "not_found")
        .await;
}
