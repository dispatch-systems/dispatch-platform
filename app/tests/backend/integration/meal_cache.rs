//! The meal-break page's cache and live results over HTTP, against the real router served
//! in-process over a loopback socket: Timecard's page, Paycom's live collection and core's
//! cache and leases together.
use crate::support::{Call, Server};
use dispatch_core::db::s;
use dispatch_paycom as paycom;
use dispatch_timecard::TimecardStore;
use serde_json::json;

#[tokio::test]
async fn meal_cache_rechecks_live_visibility_after_lease_bookkeeping_and_authorizes_hits() {
    use dispatch_paycom::PaycomStore;
    let server = Server::start().await;
    let member = server.member("member@dispatch.test").await;
    let dsp = server.dsp("Northline Logistics").await;
    let tenant = dsp.clone();
    let date: String = server
        .state
        .read(move |db| {
            let publication = db
                .collector(&tenant, paycom::PROVIDER)?
                .one("SELECT period_to FROM publications WHERE active=1", [])?
                .unwrap();
            Ok(s(&publication, "period_to").into())
        })
        .await
        .unwrap();
    let path = format!("/api/dsp/paycom/meal-breaks?date={date}");
    let before = server.send(Call::get(&path).who(&member)).await;
    assert_eq!(before.status, 200, "{}", before.body);
    // Even a populated DSP cache cannot lend its reader's authorization to another call.
    server
        .expect(Call::get(&path), 401, "sign_in_required")
        .await;
    let tenant = dsp.clone();
    let day = date.clone();
    let job = server
        .state
        .run_scoped(dsp.clone(), dispatch_timecard::DOMAIN, move |db| {
            let queued = db.enqueue_timecards(&tenant, None, "meal-cache-live")?;
            let job = db
                .claim_job("meal-cache-owner", |id, provider| {
                    id == tenant && provider == paycom::PROVIDER
                })?
                .unwrap();
            assert_eq!(job.id, s(&queued, "id"));
            let employee = db
                .collector(&tenant, paycom::PROVIDER)?
                .one(
                    "SELECT e.* FROM employees e JOIN publications p ON p.id=e.publication_id \
             WHERE p.active=1 AND e.code='E001'",
                    [],
                )?
                .unwrap();
            db.start_live(
                &job.id,
                "meal-cache-owner",
                &json!({"from":day,"to":day,"roster":[employee.clone()]}),
            )?;
            db.stage_paycom(
                &job.id,
                "meal-cache-owner",
                &employee,
                &[json!({
                    "employeeCode":"E001","date":day,"hours":9,"status":"Complete",
                    "punches":[{"in":"09:11","out":"18:11","hours":9}]
                })],
            )?;
            Ok(job.id)
        })
        .await
        .unwrap();
    let live = server.send(Call::get(&path).who(&member)).await;
    assert_eq!(live.status, 200, "{}", live.body);
    let live_row = live.body["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == "paycom:E001")
        .unwrap();
    assert_eq!(live_row["paycom"]["punches"][0]["in"], "09:11");
    server
        .state
        .run_bookkeeping(move |db| {
            db.jobs
                .exec("UPDATE jobs SET lease_until=0 WHERE id=?", [job])?;
            Ok(())
        })
        .await
        .unwrap();
    let expired = server.send(Call::get(&path).who(&member)).await;
    assert_eq!(expired.status, 200, "{}", expired.body);
    assert_eq!(
        expired.body, before.body,
        "expired live data must immediately return to the publication"
    );
}
