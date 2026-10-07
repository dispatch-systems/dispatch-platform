//! Cortex's live probes that need Cortex alone, ignored: an operator runs one at a time
//! against a copy of a DSP, with the `operator-probes` feature. Never run by CI or print
//! provider records. Each installs a registry of Cortex, as the app installs its own.
use super::*;
use crate::collections::meals::collect as collection;
use dispatch_core::collection::browser::page::call;
use tokio::time::sleep;

fn response_bytes(body: &Value) -> Result<Vec<u8>> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    if body["base64Encoded"] == true {
        STANDARD
            .decode(s(body, "body"))
            .map_err(|_| Error::new("benchmark_response_unreadable", 502))
    } else {
        Ok(s(body, "body").as_bytes().to_vec())
    }
}
fn tab_pair(value: &str) -> Result<Vec<usize>> {
    let tabs: Vec<usize> = value
        .split(',')
        .map(|part| {
            part.trim()
                .parse::<usize>()
                .map_err(|_| Error::new("benchmark_configuration_required", 400))
        })
        .collect::<Result<_>>()?;
    ensure(
        tabs.len() == 2 && tabs.iter().all(|n| *n > 0),
        "benchmark_configuration_required",
        400,
    )?;
    Ok(tabs)
}
fn adjacent_week(week: &str, delta: i64) -> Result<String> {
    let (year, number) = dispatch_core::foundation::weeks::parse_week(week)?;
    let day = chrono::NaiveDate::from_isoywd_opt(year, number, chrono::Weekday::Mon)
        .ok_or_else(|| Error::new("invalid_week", 400))?;
    let shifted = day
        .checked_add_signed(chrono::Duration::weeks(delta))
        .ok_or_else(|| Error::new("invalid_week", 400))?;
    Ok(crate::dvic::report_week(shifted))
}
/// The requests a tab's document made, from its own resource timing: data requests
/// by masked address (a path segment with a digit is an identifier; query values
/// are dropped) with their largest size and time, and other kinds counted.
const REQUESTS: &str = r#"(()=>{const mask=s=>/\d/.test(s)||s.length>32?'{id}':s;
  const rows={},other={},all=performance.getEntriesByType('resource');
  for(const e of all){
    if(!['fetch','xmlhttprequest'].includes(e.initiatorType)){other[e.initiatorType]=(other[e.initiatorType]||0)+1;continue;}
    const u=new URL(e.name),names=[...new Set(u.searchParams.keys())].sort();
    const key=u.host+u.pathname.split('/').map(mask).join('/')+(names.length?'?'+names.join('&'):'');
    const r=rows[key]||(rows[key]={count:0,maxBytes:0,maxMs:0});
    r.count++;r.maxBytes=Math.max(r.maxBytes,e.decodedBodySize||0);r.maxMs=Math.max(r.maxMs,Math.round(e.duration));}
  return {data:rows,other,entries:all.length,transferBytes:all.reduce((s,e)=>s+(e.transferSize||0),0),
    decodedBytes:all.reduce((s,e)=>s+(e.decodedBodySize||0),0)};})()"#;

// Which data requests Cortex's itinerary pages make: signs in, loads the list and one
// route's details as a collection does, and prints what each document requested.
#[tokio::test]
#[ignore = "requires an explicitly selected DSP and authenticated provider profile"]
async fn record_data_requests() -> Result<()> {
    dispatch_core::testing::install(&[&crate::COLLECTOR], &[]);
    let dsp = env_path("DISPATCH_BENCHMARK_DSP")?;
    let profile = dsp.join("state/browsers/cortex-browseros");
    let runtime = browseros::Runtime::new(
        Path::new("/opt/dispatch-browseros/0.50.5/browseros"),
        Path::new("/usr/local/libexec/dispatch-dev/bwrap"),
        &env_path("DISPATCH_BENCHMARK_WORKER")?,
        &env_path("DISPATCH_BENCHMARK_RUNS")?,
        1,
    )?;
    let browser = runtime
        .start(
            &profile,
            browseros::Mode::Windowed,
            browseros::NetworkPolicy::Hosts(&crate::BROWSER_HOSTS),
        )
        .await?;
    let mut driver = Driver::new(browser, &profile, None).await?;
    let result = async {
        let secrets = dsp.join("secrets");
        let credentials = dispatch_core::foundation::crypto::decrypt(
            &db::key_file(&secrets.join("vault.key"))?,
            &format!("{}:cortex:2", dsp.file_name().unwrap().to_str().unwrap()),
            &std::fs::read_to_string(secrets.join("cortex.enc"))?,
        )?;
        let started = Instant::now();
        let signed = driver
            .request(json!({"action":"start","credentials":credentials}))
            .await;
        eprintln!(
            "REQUESTS {}",
            json!({"signIn":signed.as_ref().map(|v|s(v,"type").to_owned()).unwrap_or_else(|e|e.code.clone()),
                "ms":started.elapsed().as_millis()})
        );
        ensure(
            signed.is_ok_and(|v| v["type"] == "ready"),
            "benchmark_verification_required",
            409,
        )?;
        let scope: Scope = serde_json::from_str(
            &std::env::var("DISPATCH_BENCHMARK_SCOPE")
                .map_err(|_| Error::new("benchmark_configuration_required", 400))?,
        )?;
        let metrics = Recorder::new(&json!({}));
        let started = Instant::now();
        let candidates = driver.candidates(&scope, &metrics).await?;
        let list = driver.browser.evaluate(&driver.page.id, REQUESTS).await?;
        eprintln!(
            "REQUESTS {}",
            json!({"page":"list","ms":started.elapsed().as_millis(),"routes":candidates.len(),"observed":list})
        );
        let candidate = candidates
            .first()
            .ok_or_else(|| Error::new("benchmark_no_routes", 409))?;
        let started = Instant::now();
        driver.meal_page(&driver.page, &scope, Some(candidate), &metrics).await?;
        let detail = driver.browser.evaluate(&driver.page.id, REQUESTS).await?;
        eprintln!(
            "REQUESTS {}",
            json!({"page":"detail","ms":started.elapsed().as_millis(),"observed":detail})
        );
        Ok(())
    }
    .await;
    driver.browser.close().await;
    result
}

/// The first request the tab makes under `path` while loading `url`, sent on as usual:
/// its method, address and the headers a page may set. Prints only header names.
async fn capture(driver: &Driver, url: &str, path: &str) -> Result<Value> {
    driver
        .page
        .command(
            "Fetch.enable",
            json!({"patterns":[{"urlPattern":format!("*{path}*"),"requestStage":"Request"}]}),
        )
        .await?;
    driver.page.start_navigation(url).await?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let request = loop {
        ensure(Instant::now() < deadline, "provider_timeout", 504)?;
        let event = driver.browser.event(&driver.page.id).await?;
        if event.is_null() {
            continue;
        }
        driver
            .page
            .command(
                "Fetch.continueRequest",
                json!({"requestId":event["requestId"]}),
            )
            .await?;
        break event["request"].clone();
    };
    driver.page.command("Fetch.disable", json!({})).await?;
    let forbidden = |name: &str| {
        let name = name.to_ascii_lowercase();
        [
            "cookie",
            "host",
            "origin",
            "referer",
            "user-agent",
            "accept-encoding",
            "connection",
            "content-length",
        ]
        .contains(&name.as_str())
            || name.starts_with("sec-")
            || name.starts_with("proxy-")
    };
    let headers = request["headers"]
        .as_object()
        .map(|h| {
            h.iter()
                .filter(|(k, _)| !forbidden(k))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<serde_json::Map<_, _>>()
        })
        .unwrap_or_default();
    eprintln!(
        "REQUESTS {}",
        json!({"captured":path,"method":request["method"],"hasBody":request["hasPostData"],
            "headers":headers.keys().collect::<Vec<_>>()})
    );
    Ok(json!({"url":request["url"],"method":request["method"],"headers":headers}))
}

/// A capture's routes by id, without the moment each was observed.
fn routes(capture: &Value) -> std::collections::BTreeMap<String, Value> {
    capture["itineraries"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|route| {
            let mut route = route.clone();
            route.as_object_mut().map(|r| r.remove("observedAt"));
            (s(&route, "id").to_owned(), route)
        })
        .collect()
}
/// Counts that describe a capture without its values.
fn totals(capture: &Value) -> Value {
    let routes = routes(capture);
    let meals = routes
        .values()
        .flat_map(|r| r["meals"].as_array().cloned().unwrap_or_default())
        .collect::<Vec<_>>();
    json!({"routes":routes.len(),
        "drivers":routes.values().map(|r|s(r,"transporterId").to_owned()).collect::<std::collections::BTreeSet<_>>().len(),
        "completeRoutes":routes.values().filter(|r|r["routeComplete"]==true).count(),
        "completeCoverage":routes.values().filter(|r|r["deliveryCoverage"]=="complete").count(),
        "meals":meals.len(),"openMeals":meals.iter().filter(|m|m["end"].is_null()).count(),
        "lastDeliveryBefore":meals.iter().filter(|m|!m["lastDelivery"].is_null()).count(),
        "firstDeliveryAfter":meals.iter().filter(|m|!m["firstDelivery"].is_null()).count()})
}
/// Where two values differ, as field paths without values or indexes.
fn differences(a: &Value, b: &Value, path: &str, out: &mut std::collections::BTreeSet<String>) {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for key in x.keys().chain(y.keys()) {
                differences(
                    x.get(key).unwrap_or(&Value::Null),
                    y.get(key).unwrap_or(&Value::Null),
                    &format!("{path}.{key}"),
                    out,
                );
            }
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
            for (p, q) in x.iter().zip(y) {
                differences(p, q, &format!("{path}[]"), out);
            }
        }
        _ if a != b => {
            out.insert(path.to_owned());
        }
        _ => {}
    }
}

// A whole day collected with one tab, as jobs have, then with two, compared route by
// route and field by field. Amazon's own list response is counted as well, so a
// route missing from both collections would still show. Prints counts and field
// names only.
#[tokio::test]
#[ignore = "requires an explicitly selected DSP and authenticated provider profile"]
async fn compare_tabs() -> Result<()> {
    dispatch_core::testing::install(&[&crate::COLLECTOR], &[]);
    let runs =
        tab_pair(&std::env::var("DISPATCH_BENCHMARK_TABS").unwrap_or_else(|_| "1,2".into()))?;
    let dsp = env_path("DISPATCH_BENCHMARK_DSP")?;
    let profile = dsp.join("state/browsers/cortex-browseros");
    let runtime = browseros::Runtime::new(
        Path::new("/opt/dispatch-browseros/0.50.5/browseros"),
        Path::new("/usr/local/libexec/dispatch-dev/bwrap"),
        &env_path("DISPATCH_BENCHMARK_WORKER")?,
        &env_path("DISPATCH_BENCHMARK_RUNS")?,
        1,
    )?;
    let browser = runtime
        .start(
            &profile,
            browseros::Mode::Windowed,
            browseros::NetworkPolicy::Hosts(&crate::BROWSER_HOSTS),
        )
        .await?;
    let mut driver = Driver::new(browser, &profile, None).await?;
    let result = async {
        let secrets = dsp.join("secrets");
        let credentials = dispatch_core::foundation::crypto::decrypt(
            &db::key_file(&secrets.join("vault.key"))?,
            &format!("{}:cortex:2", dsp.file_name().unwrap().to_str().unwrap()),
            &std::fs::read_to_string(secrets.join("cortex.enc"))?,
        )?;
        let signed = driver
            .request(json!({"action":"start","credentials":credentials}))
            .await;
        ensure(
            signed.is_ok_and(|v| v["type"] == "ready"),
            "benchmark_verification_required",
            409,
        )?;
        let scope: Scope = serde_json::from_str(
            &std::env::var("DISPATCH_BENCHMARK_SCOPE")
                .map_err(|_| Error::new("benchmark_configuration_required", 400))?,
        )?;
        // Amazon's own list for the day: the app's signed request, sent again as is.
        let origin = driver.origin.clone();
        let request = capture(
            &driver,
            &format!("{origin}{}", scope.list_path()),
            "/operations/execution/api/summaries",
        )
        .await?;
        driver.candidates(&scope, &Recorder::new(&json!({}))).await?;
        let listed = driver
            .browser
            .evaluate(
                &driver.page.id,
                &format!(
                    "fetch({},{{headers:{},credentials:'include',cache:'no-store'}}).then(r=>r.json())\
                     .then(b=>b.itinerarySummaries.filter(s=>s.companyId==={}).length)",
                    json!(request["url"]),
                    request["headers"],
                    json!(scope.provider)
                ),
            )
            .await?;
        eprintln!("PARITY {}", json!({"amazonListsRoutes":listed}));
        let mut captures = Vec::new();
        for tabs in runs {
            let metrics = Recorder::new(&json!({}));
            let started = Instant::now();
            let method = collection::MealMethod {
                tabs,
                ..collection::MealMethod::rendered()
            };
            let capture = driver
                .collect(&scope, &metrics, None, |_, _| async { Ok(()) }, &method)
                .await;
            let capture = match capture {
                Ok(capture) => capture,
                Err(error) => {
                    // Which read stalled and why: fixed labels and timings only.
                    let snapshot = serde_json::to_value(metrics.snapshot())?;
                    let hidden = driver
                        .browser
                        .command("Target.getTargets", json!({}), None)
                        .await
                        .map(|t| t["targetInfos"].as_array().map(Vec::len))
                        .ok();
                    eprintln!(
                        "PARITY {}",
                        json!({"tabs":tabs,"error":error.code,"detail":snapshot["detail"],
                            "failures":snapshot["pageReads"]["failures"],"active":snapshot["pageReads"]["active"],
                            "completed":snapshot["pageReads"]["completed"],"targets":hidden})
                    );
                    return Err(error);
                }
            };
            let reads = serde_json::to_value(metrics.snapshot())?["pageReads"].clone();
            eprintln!(
                "PARITY {}",
                json!({"tabs":tabs,"ms":started.elapsed().as_millis(),"routeReads":reads["completed"],
                    "failedReads":reads["failures"].as_array().map(Vec::len),"totals":totals(&capture)})
            );
            captures.push(routes(&capture));
        }
        let (one, two) = (&captures[0], &captures[1]);
        let mut fields = std::collections::BTreeSet::new();
        let mut differing = 0;
        for (id, route) in one {
            if let Some(other) = two.get(id) {
                differences(route, other, "", &mut fields);
                if route != other {
                    differing += 1;
                }
            }
        }
        let only_one = one.keys().filter(|id| !two.contains_key(*id)).count();
        let only_two = two.keys().filter(|id| !one.contains_key(*id)).count();
        eprintln!(
            "PARITY {}",
            json!({"compared":one.len().min(two.len()),"onlyOneTab":only_one,"onlyTwoTabs":only_two,
                "differingRoutes":differing,"differingFields":fields,
                "matchesAmazonList":listed.as_u64()==Some(one.len() as u64)&&listed.as_u64()==Some(two.len() as u64)})
        );
        Ok(())
    }
    .await;
    driver.browser.close().await;
    result
}

/// A tab's state while a route loads: visibility, the app's loading flags and its
/// recent data requests' statuses. Labels, flags and masked paths only.
const DIAGNOSE: &str = r#"(()=>{let root;const seen=new Set();
  for(const element of document.querySelectorAll('*')){const key=Object.keys(element).find(k=>k.startsWith('__reactFiber'));
    for(let fiber=element[key],depth=0;fiber&&depth<80;fiber=fiber.return,depth++){if(seen.has(fiber))break;seen.add(fiber);
      const p=fiber.memoizedProps;if(p&&Array.isArray(p.allItinerarySummaries)&&p.transporterSummary)root=p;}}
  const now=performance.now();
  return {page:/\/documentType\//.test(location.pathname)?'detail':'list',visible:document.visibilityState,focus:document.hasFocus(),
    root:!!root,loadingSummaries:root?root.isLoadingSummaries:null,details:!!(root&&root.itineraryDetails),
    loadingDetails:root?root.isLoadingItineraryDetails:null,
    requests:performance.getEntriesByType('resource').filter(e=>['fetch','xmlhttprequest'].includes(e.initiatorType)&&now-e.startTime<40000)
      .map(e=>({path:new URL(e.name).pathname.split('/').map(s=>/\d/.test(s)||s.length>32?'{id}':s).join('/'),
        status:e.responseStatus,ms:Math.round(e.duration),agoMs:Math.round(now-e.startTime)}))};})()"#;

// Two tabs collecting a day while every tab's state is sampled; prints the samples
// of any tab still loading a route after ten seconds.
#[tokio::test]
#[ignore = "requires an explicitly selected DSP and authenticated provider profile"]
async fn diagnose_tabs() -> Result<()> {
    dispatch_core::testing::install(&[&crate::COLLECTOR], &[]);
    let dsp = env_path("DISPATCH_BENCHMARK_DSP")?;
    let profile = dsp.join("state/browsers/cortex-browseros");
    let runtime = browseros::Runtime::new(
        Path::new("/opt/dispatch-browseros/0.50.5/browseros"),
        Path::new("/usr/local/libexec/dispatch-dev/bwrap"),
        &env_path("DISPATCH_BENCHMARK_WORKER")?,
        &env_path("DISPATCH_BENCHMARK_RUNS")?,
        1,
    )?;
    let browser = runtime
        .start(
            &profile,
            browseros::Mode::Windowed,
            browseros::NetworkPolicy::Hosts(&crate::BROWSER_HOSTS),
        )
        .await?;
    let mut driver = Driver::new(browser.clone(), &profile, None).await?;
    let result = async {
        let secrets = dsp.join("secrets");
        let credentials = dispatch_core::foundation::crypto::decrypt(
            &db::key_file(&secrets.join("vault.key"))?,
            &format!("{}:cortex:2", dsp.file_name().unwrap().to_str().unwrap()),
            &std::fs::read_to_string(secrets.join("cortex.enc"))?,
        )?;
        let signed = driver
            .request(json!({"action":"start","credentials":credentials}))
            .await;
        ensure(
            signed.is_ok_and(|v| v["type"] == "ready"),
            "benchmark_verification_required",
            409,
        )?;
        let scope: Scope = serde_json::from_str(
            &std::env::var("DISPATCH_BENCHMARK_SCOPE")
                .map_err(|_| Error::new("benchmark_configuration_required", 400))?,
        )?;
        let metrics = Recorder::new(&json!({}));
        let done = std::sync::atomic::AtomicBool::new(false);
        let origin = driver.origin.clone();
        let watch = async {
            let mut sessions = std::collections::BTreeMap::<String, (usize, String)>::new();
            let mut loading = std::collections::BTreeMap::<String, u32>::new();
            while !done.load(std::sync::atomic::Ordering::SeqCst) {
                sleep(Duration::from_secs(5)).await;
                let Ok(targets) = browser.command("Target.getTargets", json!({}), None).await else {
                    continue;
                };
                for target in targets["targetInfos"].as_array().into_iter().flatten() {
                    if s(target, "type") != "page" || !s(target, "url").starts_with(&origin) {
                        continue;
                    }
                    let id = s(target, "targetId").to_owned();
                    if !sessions.contains_key(&id) {
                        let Ok(attached) = browser
                            .command("Target.attachToTarget", json!({"targetId":id,"flatten":true}), None)
                            .await
                        else {
                            continue;
                        };
                        let index = sessions.len() + 1;
                        sessions.insert(id.clone(), (index, s(&attached, "sessionId").to_owned()));
                    }
                    let (index, session) = sessions[&id].clone();
                    let Ok(state) = browser.evaluate(&session, DIAGNOSE).await else {
                        continue;
                    };
                    let stuck = state["page"] == "detail"
                        && (state["root"] != true || state["loadingSummaries"] != false
                            || state["details"] != true || state["loadingDetails"] != false);
                    let count = loading.entry(id.clone()).or_default();
                    *count = if stuck { *count + 1 } else { 0 };
                    if *count >= 2 {
                        eprintln!("DIAGNOSE {}", json!({"tab":index,"state":state}));
                    }
                }
            }
        };
        let collect = async {
            let result = driver
                .collect(&scope, &metrics, None, |_, _| async { Ok(()) }, &collection::MealMethod::rendered())
                .await;
            done.store(true, std::sync::atomic::Ordering::SeqCst);
            result
        };
        let (_, collected) = tokio::join!(watch, collect);
        let snapshot = serde_json::to_value(metrics.snapshot())?;
        eprintln!(
            "DIAGNOSE {}",
            json!({"outcome":collected.as_ref().map(|_|"ok".to_owned()).unwrap_or_else(|e|e.code.clone()),
                "completed":snapshot["pageReads"]["completed"],"detail":snapshot["detail"]})
        );
        collected.map(|_| ())
    }
    .await;
    browser.close().await;
    result
}

/// Records, in the page, what a download would do without the network: blobs given
/// an object URL, anchors clicked and windows opened. Blob contents are kept for
/// the benchmark to save; addresses are masked.
const DOWNLOAD_HOOK: &str = include_str!("../../probes/download_hook.js");

/// A paused response, described without its contents: kind, method, masked address,
/// status, error and the headers that say what it is.
fn describe_response(event: &Value) -> Value {
    let mask = |segment: &str| {
        if segment.chars().any(|c| c.is_ascii_digit()) || segment.len() > 32 {
            "{id}".to_owned()
        } else {
            segment.to_owned()
        }
    };
    let url = url::Url::parse(s(&event["request"], "url")).ok();
    let mut headers = serde_json::Map::new();
    for header in event["responseHeaders"].as_array().into_iter().flatten() {
        let name = s(header, "name").to_ascii_lowercase();
        let value = s(header, "value");
        let kept = match name.as_str() {
            "content-type" | "content-length" | "cache-control" | "x-amz-request-id" => {
                Some(value.to_owned())
            }
            "content-disposition" => Some(format!(
                "{}{}",
                if value.contains("attachment") {
                    "attachment"
                } else {
                    "inline"
                },
                std::path::Path::new(value.trim_end_matches('"'))
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| format!(" .{e}"))
                    .unwrap_or_default()
            )),
            "location" => url::Url::parse(value).ok().map(|u| {
                format!(
                    "{}{}",
                    u.host_str().unwrap_or(""),
                    u.path().split('/').map(mask).collect::<Vec<_>>().join("/")
                )
            }),
            _ => None,
        };
        if let Some(kept) = kept {
            headers.insert(name, json!(kept));
        }
    }
    json!({
        "type": event["resourceType"],
        "method": event["request"]["method"],
        "host": url.as_ref().and_then(|u| u.host_str()).unwrap_or(""),
        "path": url.as_ref().map(|u| u.path().split('/').map(mask).collect::<Vec<_>>().join("/")).unwrap_or_default(),
        "queryKeys": url.as_ref().map(|u| {
            let mut k: Vec<_> = u.query_pairs().map(|(k, _)| k.into_owned()).collect();
            k.sort();
            k.dedup();
            k
        }).unwrap_or_default(),
        "status": event["responseStatusCode"],
        "error": event["responseErrorReason"],
        "headers": headers,
    })
}

/// Saves a JSON capture privately and describes it: name, size and type.
fn save_capture(
    output: &Path,
    name: &str,
    index: usize,
    kind: &str,
    bytes: &[u8],
) -> Result<Value> {
    let path = output.join(format!("{name}-{index}.json"));
    {
        use std::{io::Write, os::unix::fs::OpenOptionsExt};
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)?
            .write_all(bytes)?;
    }
    db::private_file(&path, false)?;
    Ok(json!({"file":path.file_name().and_then(|f|f.to_str()),"bytes":bytes.len(),"kind":kind}))
}

/// The scorecard data API, asked from the page with its cookies: the request path's
/// shape, each weekly dataset's row count and the JavaScript types of its fields, and
/// how many rows summary datasets have for weeks past and not yet posted. Field names,
/// types, counts and shapes only; the path segment and DSP parameter go back to the
/// benchmark unprinted so it can repeat one request itself.
const API_SHAPES: &str = include_str!("../../probes/api_shapes.js");

// What a collector reading the scorecard API needs to know: signs in, lets the
// overview resolve the company, records the API's shape, each dataset's fields and
// row counts and its answers for other weeks, then repeats one request from this
// process over plain HTTP with the browser's cookies. Prints shapes, names, types
// and counts only.
#[tokio::test]
#[ignore = "requires an explicitly selected DSP and authenticated provider profile"]
async fn probe_scorecard_api() -> Result<()> {
    dispatch_core::testing::install(&[&crate::COLLECTOR], &[]);
    let dsp = env_path("DISPATCH_BENCHMARK_DSP")?;
    let week = std::env::var("DISPATCH_BENCHMARK_WEEK")
        .map_err(|_| Error::new("benchmark_configuration_required", 400))?;
    let station = std::env::var("DISPATCH_BENCHMARK_STATION")
        .map_err(|_| Error::new("benchmark_configuration_required", 400))?;
    // Amazon's week runs Sunday to Saturday; its daily datasets take those dates.
    let (first_day, last_day) = {
        let (year, number) = week
            .split_once("-W")
            .ok_or_else(|| Error::new("benchmark_configuration_required", 400))?;
        let monday = chrono::NaiveDate::from_isoywd_opt(
            year.parse()
                .map_err(|_| Error::new("benchmark_configuration_required", 400))?,
            number
                .parse()
                .map_err(|_| Error::new("benchmark_configuration_required", 400))?,
            chrono::Weekday::Mon,
        )
        .ok_or_else(|| Error::new("benchmark_configuration_required", 400))?;
        (
            (monday - chrono::Duration::days(1)).to_string(),
            (monday + chrono::Duration::days(5)).to_string(),
        )
    };
    let profile = dsp.join("state/browsers/cortex-browseros");
    let runtime = browseros::Runtime::new(
        Path::new("/opt/dispatch-browseros/0.50.5/browseros"),
        Path::new("/usr/local/libexec/dispatch-dev/bwrap"),
        &env_path("DISPATCH_BENCHMARK_WORKER")?,
        &env_path("DISPATCH_BENCHMARK_RUNS")?,
        1,
    )?;
    let browser = runtime
        .start(
            &profile,
            browseros::Mode::Windowed,
            browseros::NetworkPolicy::Hosts(&crate::BROWSER_HOSTS),
        )
        .await?;
    let mut driver = Driver::new(browser, &profile, None).await?;
    let result = async {
        let secrets = dsp.join("secrets");
        let credentials = dispatch_core::foundation::crypto::decrypt(
            &db::key_file(&secrets.join("vault.key"))?,
            &format!("{}:cortex:2", dsp.file_name().unwrap().to_str().unwrap()),
            &std::fs::read_to_string(secrets.join("cortex.enc"))?,
        )?;
        let signed = driver
            .request(json!({"action":"start","credentials":credentials}))
            .await;
        ensure(
            signed.is_ok_and(|v| v["type"] == "ready"),
            "benchmark_verification_required",
            409,
        )?;
        let driver = &driver;
        let page = &driver.page;
        let origin = driver.origin.clone();
        page.start_navigation(&format!("{origin}/performance?pageId=dsp_dashboard_overview"))
            .await?;
        // Until Cortex has chosen the station and company and the page asked for data.
        let started = Instant::now();
        let company = loop {
            ensure(started.elapsed() < Duration::from_secs(45), "cortex_content_incomplete", 502)?;
            sleep(Duration::from_millis(500)).await;
            let frame = page.frame().await?;
            let Ok(url) = url::Url::parse(s(&frame, "url")) else { continue };
            let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
            if query.get("station") == Some(&station)
                && let Some(company) = query.get("companyId")
                && driver
                    .browser
                    .evaluate(
                        &page.id,
                        "performance.getEntriesByType('resource')\
                         .filter(e=>e.name.includes('/performance/api/')&&e.name.includes('getData')).length",
                    )
                    .await
                    .ok()
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0)
                    > 0
            {
                break company.clone();
            }
        };
        let adjacent = [1, 2, -1, -8, -18, -30].into_iter()
            .map(|delta| Ok(json!({"delta":delta,"week":adjacent_week(&week, delta)?})))
            .collect::<Result<Vec<_>>>()?;
        let input = json!({"company":company,"week":week,"station":station,
            "firstDay":first_day,"lastDay":last_day,"adjacentWeeks":adjacent});
        driver.browser.evaluate(&page.id, &call(API_SHAPES, &input)).await?;
        let started = Instant::now();
        let probe = loop {
            sleep(Duration::from_millis(500)).await;
            let state = driver
                .browser
                .evaluate(&page.id, "JSON.stringify(globalThis.__dispatchApi||{})")
                .await
                .ok()
                .and_then(|v| serde_json::from_str::<Value>(v.as_str().unwrap_or("{}")).ok())
                .unwrap_or_default();
            if state["done"] == true || started.elapsed() > Duration::from_secs(120) {
                break state;
            }
        };
        eprintln!(
            "WEEKLY_SCORECARD_API {}",
            json!({"ms":started.elapsed().as_millis(),"error":probe.get("error"),"probe":probe.get("out").or(probe.get("partial"))})
        );
        // The same request from this process: the browser's cookies and user agent, nothing else.
        let (Some(segment), Some(dsp_param)) = (probe["segment"].as_str(), probe["dsp"].as_str()) else {
            return Ok(());
        };
        let cookies = driver.browser.command("Storage.getCookies", json!({}), None).await?;
        let version = driver.browser.command("Browser.getVersion", json!({}), None).await?;
        let jar = reqwest::cookie::Jar::default();
        let mut kept = 0;
        for cookie in cookies["cookies"].as_array().into_iter().flatten() {
            let domain = s(cookie, "domain");
            let host = domain.trim_start_matches('.');
            if !(host == "logistics.amazon.com" || host == "amazon.com") {
                continue;
            }
            let Ok(url) = url::Url::parse(&format!("https://{host}/")) else { continue };
            let mut line = format!("{}={}; Path={}", s(cookie, "name"), s(cookie, "value"), s(cookie, "path"));
            if domain.starts_with('.') {
                line.push_str(&format!("; Domain={host}"));
            }
            if cookie["secure"] == true {
                line.push_str("; Secure");
            }
            jar.add_cookie_str(&line, &url);
            kept += 1;
        }
        let client = reqwest::Client::builder()
            .cookie_provider(std::sync::Arc::new(jar))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(s(&version, "userAgent"))
            .build()
            .map_err(|_| Error::new("browser_unavailable", 503))?;
        let mut target = url::Url::parse(&format!("{origin}/performance/api/{segment}/getData"))
            .map_err(|_| Error::new("egress_denied", 403))?;
        target
            .query_pairs_mut()
            .append_pair("dataSetId", "dsp_weekly_cdf")
            .append_pair("dsp", dsp_param)
            .append_pair("from", &week)
            .append_pair("station", &station)
            .append_pair("timeFrame", "Weekly")
            .append_pair("to", &week);
        let started = Instant::now();
        let response = client
            .get(target)
            .header("Accept", "application/json, text/plain, */*")
            .timeout(Duration::from_secs(30))
            .send()
            .await;
        let summary = match response {
            Ok(response) => {
                let status = response.status().as_u16();
                let content_type = response
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_owned();
                let text = response.text().await.unwrap_or_default();
                let rows = serde_json::from_str::<Value>(&text)
                    .ok()
                    .and_then(|v| v["tableData"].as_object().and_then(|t| t.values().next().cloned()))
                    .and_then(|t| t["rows"].as_array().map(Vec::len));
                json!({"status":status,"contentType":content_type,"bytes":text.len(),"rows":rows})
            }
            Err(error) => json!({"error":error.to_string().split(':').next().unwrap_or("request_failed")}),
        };
        eprintln!(
            "WEEKLY_SCORECARD_API {}",
            json!({"http":summary,"cookiesKept":kept,"ms":started.elapsed().as_millis()})
        );
        Ok(())
    }
    .await;
    driver.browser.close().await;
    result
}

/// The shape of a JSON value without its contents. Objects give each key's shape;
/// arrays of objects give, per key, how many items carry it, how many fill it, the
/// kinds of value seen and the range of string lengths, plus the shape of the first
/// nested object or array under each key. Strings are classed, never shown.
fn shape(value: &Value, depth: usize) -> Value {
    fn class(text: &str) -> &'static str {
        let bytes = text.as_bytes();
        if text.is_empty() {
            "empty"
        } else if text.len() == 36 && text.bytes().filter(|b| *b == b'-').count() == 4 {
            "uuid"
        } else if text.len() == 10 && bytes[4] == b'-' && bytes[7] == b'-' {
            "date"
        } else if text.len() >= 16
            && bytes[4] == b'-'
            && bytes[7] == b'-'
            && (bytes[10] == b'T' || bytes[10] == b' ')
        {
            "datetime"
        } else if bytes
            .iter()
            .all(|b| b.is_ascii_digit() || *b == b'.' || *b == b'-' || *b == b'+')
        {
            "digits"
        } else if bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
        {
            "alnum"
        } else if bytes.contains(&b'@') {
            "email"
        } else {
            "text"
        }
    }
    fn kind(value: &Value) -> String {
        match value {
            Value::Null => "null".into(),
            Value::Bool(_) => "bool".into(),
            Value::Number(_) => "number".into(),
            Value::String(text) => format!("string:{}", class(text)),
            Value::Array(_) => "array".into(),
            Value::Object(_) => "object".into(),
        }
    }
    fn filled(value: &Value) -> bool {
        match value {
            Value::Null => false,
            Value::String(text) => !text.trim().is_empty(),
            Value::Array(items) => !items.is_empty(),
            Value::Object(fields) => !fields.is_empty(),
            _ => true,
        }
    }
    type Field = (
        usize,
        usize,
        std::collections::BTreeSet<String>,
        usize,
        usize,
    );
    match value {
        Value::Object(fields) if depth > 7 => {
            json!({"object":fields.len(),"keys":fields.keys().take(80).collect::<Vec<_>>()})
        }
        Value::Object(fields) => {
            let mut out = serde_json::Map::new();
            for (key, child) in fields.iter().take(150) {
                out.insert(key.clone(), shape(child, depth + 1));
            }
            json!({"object":out})
        }
        Value::Array(items) => {
            let objects: Vec<&serde_json::Map<String, Value>> =
                items.iter().filter_map(Value::as_object).collect();
            if objects.is_empty() {
                return json!({"array":items.len(),"item":items.first().map(|v| shape(v, depth + 1))});
            }
            let mut fields: std::collections::BTreeMap<String, Field> = Default::default();
            let mut nested = serde_json::Map::new();
            for object in objects.iter().take(400) {
                for (key, child) in object.iter() {
                    let entry = fields.entry(key.clone()).or_insert((
                        0,
                        0,
                        Default::default(),
                        usize::MAX,
                        0,
                    ));
                    entry.0 += 1;
                    if filled(child) {
                        entry.1 += 1;
                    }
                    entry.2.insert(kind(child));
                    if let Value::String(text) = child {
                        entry.3 = entry.3.min(text.chars().count());
                        entry.4 = entry.4.max(text.chars().count());
                    }
                    if depth < 7
                        && !nested.contains_key(key)
                        && filled(child)
                        && (child.is_object() || child.is_array())
                    {
                        nested.insert(key.clone(), shape(child, depth + 1));
                    }
                }
            }
            let fields: serde_json::Map<String, Value> = fields
                .into_iter()
                .map(|(key, (present, filled, kinds, min, max))| {
                    let mut record = json!({"present":present,"filled":filled,"kinds":kinds});
                    if max > 0 {
                        record["len"] = json!([min, max]);
                    }
                    (key, record)
                })
                .collect();
            json!({"array":items.len(),"objects":objects.len(),"fields":fields,"nested":nested})
        }
        other => json!(kind(other)),
    }
}
/// A path segment with a digit, or a long one, is an identifier.
fn mask_segment(segment: &str) -> String {
    if segment.chars().any(|c| c.is_ascii_digit()) || segment.len() > 32 {
        "{id}".to_owned()
    } else {
        segment.to_owned()
    }
}
/// An address as printed: masked path and the names of its query parameters.
fn masked_url(url: &str) -> Value {
    url::Url::parse(url)
        .map(|u| {
            let mut keys: Vec<_> = u.query_pairs().map(|(k, _)| k.into_owned()).collect();
            keys.sort();
            json!({"path":u.path().split('/').map(mask_segment).collect::<Vec<_>>().join("/"),"queryKeys":keys})
        })
        .unwrap_or_else(|_| json!({"path":"(unparsed)"}))
}
/// Loads `url` in the driver's tab while every data response under
/// /operations/execution/api/ is paused, read and let go. Prints each response's
/// masked address, size and shape, saves its body under `output` when given, and
/// returns the parsed bodies by masked path.
async fn load_routes_page(
    driver: &Driver,
    output: Option<&Path>,
    name: &'static str,
    url: &str,
    saved: &mut usize,
) -> Result<Vec<(String, Option<Value>)>> {
    let page = &driver.page;
    let patterns: Vec<Value> = ["XHR", "Fetch"]
        .iter()
        .map(|kind| json!({"urlPattern":"*/operations/execution/api/*","resourceType":kind,"requestStage":"Response"}))
        .collect();
    page.command("Fetch.enable", json!({"patterns":patterns}))
        .await?;
    let started = Instant::now();
    page.start_navigation(url).await?;
    let mut bodies: Vec<(Value, Vec<u8>)> = Vec::new();
    let mut last_event = Instant::now();
    while started.elapsed() < Duration::from_secs(60) {
        let event = driver.browser.event(&page.id).await?;
        if event.is_null() {
            // Quiet for a while after the first response: the page has what it needs.
            if !bodies.is_empty() && last_event.elapsed() > Duration::from_secs(6) {
                break;
            }
            continue;
        }
        last_event = Instant::now();
        if event["responseStatusCode"].is_null() {
            let _ = page
                .command(
                    "Fetch.continueRequest",
                    json!({"requestId":event["requestId"]}),
                )
                .await;
            continue;
        }
        let described = describe_response(&event);
        let body = page
            .command(
                "Fetch.getResponseBody",
                json!({"requestId":event["requestId"]}),
            )
            .await;
        if page
            .command(
                "Fetch.continueResponse",
                json!({"requestId":event["requestId"]}),
            )
            .await
            .is_err()
        {
            let _ = page
                .command(
                    "Fetch.continueRequest",
                    json!({"requestId":event["requestId"]}),
                )
                .await;
        }
        let bytes = match body {
            Ok(body) => response_bytes(&body)?,
            Err(error) => {
                eprintln!(
                    "ROUTES {}",
                    json!({"page":name,"response":described,"bodyError":error.code})
                );
                continue;
            }
        };
        bodies.push((described, bytes));
    }
    let _ = page.command("Fetch.disable", json!({})).await;
    let frame = page.frame().await?;
    eprintln!(
        "ROUTES {}",
        json!({"page":name,"ms":started.elapsed().as_millis(),"responses":bodies.len(),
            "landed":masked_url(s(&frame, "url"))})
    );
    let mut parsed = Vec::new();
    for (described, bytes) in bodies {
        let value: Option<Value> = serde_json::from_slice(&bytes).ok();
        let mut record = json!({"page":name,"response":described,"bytes":bytes.len()});
        if let Some(value) = &value {
            record["shape"] = shape(value, 0);
        }
        if let Some(output) = output {
            *saved += 1;
            record["saved"] = save_capture(output, name, *saved, "application/json", &bytes)?;
        }
        eprintln!("ROUTES {}", serde_json::to_string(&record)?);
        parsed.push((s(&record["response"], "path").to_owned(), value));
    }
    Ok(parsed)
}

/// Takes every data response under /operations/execution/api/ the tab receives in
/// the next ten seconds, printing and saving each as `load_routes_page` does, and
/// returns where the tab landed with the masked paths seen.
async fn capture_routes_responses(
    driver: &Driver,
    output: Option<&Path>,
    label: &'static str,
    saved: &mut usize,
) -> Result<Value> {
    let page = &driver.page;
    let patterns: Vec<Value> = ["XHR", "Fetch"]
        .iter()
        .map(|kind| json!({"urlPattern":"*/operations/execution/api/*","resourceType":kind,"requestStage":"Response"}))
        .collect();
    page.command("Fetch.enable", json!({"patterns":patterns}))
        .await?;
    let started = Instant::now();
    let mut seen = Vec::new();
    while started.elapsed() < Duration::from_secs(10) {
        let event = driver.browser.event(&page.id).await?;
        if event.is_null() {
            continue;
        }
        if event["responseStatusCode"].is_null() {
            let _ = page
                .command(
                    "Fetch.continueRequest",
                    json!({"requestId":event["requestId"]}),
                )
                .await;
            continue;
        }
        let described = describe_response(&event);
        let body = page
            .command(
                "Fetch.getResponseBody",
                json!({"requestId":event["requestId"]}),
            )
            .await
            .ok();
        if page
            .command(
                "Fetch.continueResponse",
                json!({"requestId":event["requestId"]}),
            )
            .await
            .is_err()
        {
            let _ = page
                .command(
                    "Fetch.continueRequest",
                    json!({"requestId":event["requestId"]}),
                )
                .await;
        }
        let mut record = json!({"page":label,"response":described});
        if let Some(body) = body {
            let bytes = response_bytes(&body)?;
            record["bytes"] = json!(bytes.len());
            if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
                record["shape"] = shape(&value, 0);
            }
            if let Some(output) = output {
                *saved += 1;
                record["saved"] = save_capture(output, label, *saved, "application/json", &bytes)?;
            }
        }
        eprintln!("ROUTES {}", serde_json::to_string(&record)?);
        seen.push(s(&record["response"], "path").to_owned());
    }
    let _ = page.command("Fetch.disable", json!({})).await;
    let frame = page.frame().await?;
    Ok::<_, Error>(json!({"landed":masked_url(s(&frame, "url")),"responses":seen}))
}

/// Surveys of the routes page: controls by kind, presses, and what appeared. Fixed
/// interface words only; other text is reported as its length.
const ROUTES_SURVEY: &str = include_str!("../../probes/routes_survey.js");
async fn survey_routes_page(driver: &Driver, input: Value) -> Result<Value> {
    driver
        .browser
        .evaluate(&driver.page.id, &call(ROUTES_SURVEY, &input))
        .await
}

// What the route pages are built from: signs in, opens the itinerary list and one
// route's details, then the newer routes page, and takes each data response as the
// browser receives it. Prints masked addresses, statuses, sizes and the shape of each
// body: key names, value kinds, fill counts and string lengths, never a value. Bodies
// are saved under DISPATCH_BENCHMARK_OUTPUT when it is set, for sizing, and are the
// operator's to delete.
#[tokio::test]
#[ignore = "requires an explicitly selected DSP and authenticated provider profile"]
async fn probe_routes_api() -> Result<()> {
    dispatch_core::testing::install(&[&crate::COLLECTOR], &[]);
    let dsp = env_path("DISPATCH_BENCHMARK_DSP")?;
    let output = std::env::var_os("DISPATCH_BENCHMARK_OUTPUT").map(PathBuf::from);
    let scope: Scope = serde_json::from_str(
        &std::env::var("DISPATCH_BENCHMARK_SCOPE")
            .map_err(|_| Error::new("benchmark_configuration_required", 400))?,
    )?;
    scope.validate()?;
    let profile = dsp.join("state/browsers/cortex-browseros");
    let runtime = browseros::Runtime::new(
        Path::new("/opt/dispatch-browseros/0.50.5/browseros"),
        Path::new("/usr/local/libexec/dispatch-dev/bwrap"),
        &env_path("DISPATCH_BENCHMARK_WORKER")?,
        &env_path("DISPATCH_BENCHMARK_RUNS")?,
        1,
    )?;
    let browser = runtime
        .start(
            &profile,
            browseros::Mode::Windowed,
            browseros::NetworkPolicy::Hosts(&crate::BROWSER_HOSTS),
        )
        .await?;
    let mut driver = Driver::new(browser, &profile, None).await?;
    let result = async {
        let secrets = dsp.join("secrets");
        let credentials = dispatch_core::foundation::crypto::decrypt(
            &db::key_file(&secrets.join("vault.key"))?,
            &format!("{}:cortex:2", dsp.file_name().unwrap().to_str().unwrap()),
            &std::fs::read_to_string(secrets.join("cortex.enc"))?,
        )?;
        let signed = driver
            .request(json!({"action":"start","credentials":credentials}))
            .await;
        eprintln!(
            "ROUTES {}",
            json!({"signIn":signed.as_ref().map(|v|s(v,"type").to_owned()).unwrap_or_else(|e|e.code.clone())})
        );
        ensure(
            signed.is_ok_and(|v| v["type"] == "ready"),
            "benchmark_verification_required",
            409,
        )?;
        let driver = &driver;
        let output = output.as_deref();
        let origin = driver.origin.clone();
        let mut saved = 0usize;
        // Where the unscoped itinerary address lands, which discovery starts from.
        let mut unscoped = url::form_urlencoded::Serializer::new(String::new());
        unscoped
            .append_pair("navMenuVariant", "external")
            .append_pair("selectedDay", &scope.date);
        load_routes_page(
            driver,
            None,
            "unscoped",
            &format!("{origin}/operations/execution/itineraries?{}", unscoped.finish()),
            &mut saved,
        )
        .await?;
        // The list the meal collector reads, then the first route's details.
        let list = load_routes_page(
            driver,
            output,
            "list",
            &format!("{origin}{}", scope.list_path()),
            &mut saved,
        )
        .await?;
        let first_route = list
            .iter()
            .filter_map(|(_, value)| value.as_ref())
            .find_map(|value| {
                value["itinerarySummaries"]
                    .as_array()?
                    .iter()
                    .find(|summary| {
                        scope.provider == "ALL_DRIVERS"
                            || summary["companyId"] == json!(scope.provider)
                    })
                    .and_then(|summary| summary["itineraryId"].as_str())
                    .map(str::to_owned)
            });
        if let Some(id) = &first_route {
            load_routes_page(
                driver,
                output,
                "detail",
                &format!("{origin}{}", scope.detail_path(id)),
                &mut saved,
            )
            .await?;
        } else {
            eprintln!(
                "ROUTES {}",
                json!({"page":"detail","skipped":"no_route_in_list"})
            );
        }
        // The newer routes page with the same parameters, at a desktop width: what it
        // asks for, where it lands, its controls, what pressing the first route card
        // loads, and what its unlabeled toolbar buttons open with downloads hooked.
        let mut query = url::form_urlencoded::Serializer::new(String::new());
        query
            .append_pair("navMenuVariant", "external")
            .append_pair("provider", &scope.provider)
            .append_pair("selectedDay", &scope.date)
            .append_pair("serviceAreaId", &scope.service_area_id);
        let routes_url = format!(
            "{origin}/operations/execution/dv/routes?{}",
            query.finish()
        );
        let page = &driver.page;
        page.command(
            "Emulation.setDeviceMetricsOverride",
            json!({"width":1600,"height":1000,"deviceScaleFactor":1,"mobile":false}),
        )
        .await?;
        load_routes_page(driver, output, "dv_routes", &routes_url, &mut saved).await?;
        sleep(Duration::from_secs(3)).await;
        let links = survey_routes_page(driver, json!({"action":"links"})).await?;
        let links: Vec<Value> = links
            .as_array()
            .into_iter()
            .flatten()
            .map(|l| masked_url(l.as_str().unwrap_or("")))
            .collect();
        eprintln!(
            "ROUTES {}",
            json!({"page":"dv_routes","links":links,
                "downloadControls":survey_routes_page(driver, json!({"action":"downloadControls"})).await?,
                "controls":survey_routes_page(driver, json!({"action":"controls"})).await?})
        );
        let hooked = driver.browser.evaluate(&page.id, DOWNLOAD_HOOK).await?;
        let pressed = survey_routes_page(driver, json!({"action":"pressCard"})).await?;
        eprintln!(
            "ROUTES {}",
            json!({"page":"dv_route","hooked":hooked,"press":pressed})
        );
        let after = capture_routes_responses(driver, output, "dv_route", &mut saved).await?;
        eprintln!(
            "ROUTES {}",
            json!({"page":"dv_route","after":after,
                "controls":survey_routes_page(driver, json!({"action":"controls","selector":"button,[role=button],a,[role=tab]"})).await?})
        );
        for index in 0..6 {
            let outcome =
                survey_routes_page(driver, json!({"action":"pressIcon","index":index})).await?;
            if outcome["done"] == true || !outcome.is_object() {
                eprintln!("ROUTES {}", json!({"page":"dv_route","icons":outcome}));
                break;
            }
            sleep(Duration::from_millis(1500)).await;
            eprintln!(
                "ROUTES {}",
                json!({"page":"dv_route","icon":index,
                    "appeared":survey_routes_page(driver, json!({"action":"appeared"})).await?,
                    "downloads":survey_routes_page(driver, json!({"action":"downloads"})).await?})
            );
            for kind in ["keyDown", "keyUp"] {
                let _ = page
                    .command(
                        "Input.dispatchKeyEvent",
                        json!({"type":kind,"key":"Escape","code":"Escape","windowsVirtualKeyCode":27}),
                    )
                    .await;
            }
            sleep(Duration::from_millis(500)).await;
        }
        let _ = page
            .command("Emulation.clearDeviceMetricsOverride", json!({}))
            .await;
        Ok(())
    }
    .await;
    driver.browser.close().await;
    result
}

// One day's meal evidence read by the method in DISPATCH_BENCHMARK_METHOD (JSON; the
// current default when unset), with the browser in DISPATCH_BENCHMARK_MODE. Run each in
// its own cgroup (systemd-run --user --scope). Prints times, passes, CPU, memory, bytes,
// counts and a digest of each route's record; never a value. Its own deadline closes the
// browser, so a slow run never has to be killed.
#[tokio::test]
#[ignore = "requires an explicitly selected DSP and authenticated provider profile"]
async fn measure_meal_method() -> Result<()> {
    dispatch_core::testing::install(&[&crate::COLLECTOR], &[]);
    use collection::MealMethod;
    use dispatch_core::collection::browser::egress::counted::{RECEIVED, SENT};
    let dsp = env_path("DISPATCH_BENCHMARK_DSP")?;
    let method: MealMethod = match std::env::var("DISPATCH_BENCHMARK_METHOD").as_deref() {
        Ok(text) if !text.is_empty() => serde_json::from_str(text)?,
        _ => MealMethod::default(),
    };
    let mode = if std::env::var("DISPATCH_BENCHMARK_MODE").as_deref() == Ok("headless") {
        browseros::Mode::Headless
    } else {
        browseros::Mode::Windowed
    };
    let scope: Scope = serde_json::from_str(
        &std::env::var("DISPATCH_BENCHMARK_SCOPE")
            .map_err(|_| Error::new("benchmark_configuration_required", 400))?,
    )?;
    let profile = dsp.join("state/browsers/cortex-browseros");
    let runtime = browseros::Runtime::new(
        Path::new("/opt/dispatch-browseros/0.50.5/browseros"),
        Path::new("/usr/local/libexec/dispatch-dev/bwrap"),
        &env_path("DISPATCH_BENCHMARK_WORKER")?,
        &env_path("DISPATCH_BENCHMARK_RUNS")?,
        1,
    )?;
    let cgroup = own_cgroup().ok_or_else(|| Error::new("benchmark_cgroup_required", 400))?;
    let browser = runtime
        .start(
            &profile,
            mode,
            browseros::NetworkPolicy::Hosts(&crate::BROWSER_HOSTS),
        )
        .await?;
    let mut driver = Driver::new(browser, &profile, None).await?;
    let result = async {
        let secrets = dsp.join("secrets");
        let credentials = dispatch_core::foundation::crypto::decrypt(
            &db::key_file(&secrets.join("vault.key"))?,
            &format!("{}:cortex:2", dsp.file_name().unwrap().to_str().unwrap()),
            &std::fs::read_to_string(secrets.join("cortex.enc"))?,
        )?;
        let signed = driver
            .request(json!({"action":"start","credentials":credentials}))
            .await;
        ensure(
            signed.is_ok_and(|v| v["type"] == "ready"),
            "benchmark_verification_required",
            409,
        )?;
        let cpu_before = cgroup_cpu_usec(&cgroup);
        let mut peak = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(cgroup.join("memory.peak"))?;
        {
            use std::io::Write;
            let _ = peak.write_all(b"reset\n");
        }
        let (sent, received) = (
            SENT.load(std::sync::atomic::Ordering::Relaxed),
            RECEIVED.load(std::sync::atomic::Ordering::Relaxed),
        );
        let pid = driver.browser.process_id();
        let sampling = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let sampling_guard = Sampling(sampling.clone());
        let sampler = {
            let sampling = sampling.clone();
            tokio::task::spawn_blocking(move || {
                let (mut max_pss, mut sum_pss, mut samples) = (0u64, 0u64, 0u64);
                while sampling.load(std::sync::atomic::Ordering::Relaxed) {
                    if let Some(memory) = dispatch_core::collection::metrics::memory(pid) {
                        max_pss = max_pss.max(memory.pss);
                        sum_pss += memory.pss;
                        samples += 1;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(250));
                }
                (max_pss, sum_pss / samples.max(1))
            })
        };
        let metrics = Recorder::new(&json!({}));
        let passes = std::sync::atomic::AtomicUsize::new(0);
        let started = Instant::now();
        let collected = tokio::time::timeout(
            Duration::from_secs(600),
            driver.collect(
                &scope,
                &metrics,
                None,
                |_, message: String| {
                    if message.starts_with("Checking source changes") {
                        passes.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    async { Ok(()) }
                },
                &method,
            ),
        )
        .await
        .map_err(|_| Error::new("benchmark_deadline", 504))
        .and_then(|result| result);
        let collect_ms = started.elapsed().as_millis();
        let cpu = cgroup_cpu_usec(&cgroup) - cpu_before;
        drop(sampling_guard);
        let (max_pss, avg_pss) = sampler.await.unwrap_or((0, 0));
        let capture = collected?;
        let peak_bytes: u64 = {
            use std::io::{Read, Seek};
            let mut text = String::new();
            peak.seek(std::io::SeekFrom::Start(0))?;
            peak.read_to_string(&mut text)?;
            text.trim().parse().unwrap_or(0)
        };
        let mut routes: Vec<Value> = capture["itineraries"].as_array().cloned().unwrap_or_default();
        routes.sort_by_key(|r| s(r, "id").to_owned());
        let records: Vec<Value> = routes
            .iter()
            .map(|r| {
                let mut r = r.clone();
                if let Some(object) = r.as_object_mut() {
                    object.remove("observedAt");
                    object.remove("sourceUrl");
                }
                json!({"route": digest(s(&r, "id")), "record": digest(&canonical(&r).to_string()),
                    "meals": r["meals"].as_array().map_or(0, Vec::len),
                    "bounded": r["meals"].as_array().map_or(0, |m| m.iter().filter(|m| !m["lastDelivery"].is_null()).count()),
                    "coverage": r["deliveryCoverage"], "complete": r["routeComplete"]})
            })
            .collect();
        let snapshot = serde_json::to_value(metrics.snapshot())?;
        let pages = &snapshot["pageReads"];
        eprintln!(
            "BENCH {}",
            json!({
                "method": method,
                "mode": if matches!(mode, browseros::Mode::Headless) {"headless"} else {"windowed"},
                "date": scope.date,
                "collectMs": collect_ms,
                "passes": passes.load(std::sync::atomic::Ordering::Relaxed),
                "cpuSeconds": (cpu as f64 / 1e6),
                "cgroupPeakMB": peak_bytes / 1_048_576,
                "browserPeakPssMB": max_pss / 1_048_576,
                "browserAvgPssMB": avg_pss / 1_048_576,
                "sentKB": (SENT.load(std::sync::atomic::Ordering::Relaxed) - sent) / 1024,
                "receivedKB": (RECEIVED.load(std::sync::atomic::Ordering::Relaxed) - received) / 1024,
                "routes": routes.len(),
                "meals": records.iter().map(|r| r["meals"].as_u64().unwrap_or(0)).sum::<u64>(),
                "bounded": records.iter().map(|r| r["bounded"].as_u64().unwrap_or(0)).sum::<u64>(),
                "coverageComplete": records.iter().filter(|r| r["coverage"] == "complete").count(),
                "pages": {"completed": pages["completed"], "retries": pages["retries"], "totalMs": pages["totalMs"]},
                "detail": snapshot["detail"],
                "digest": digest(&records.iter().map(|r| s(r, "record")).collect::<Vec<_>>().join(",")),
                "records": records,
            })
        );
        Ok(())
    }
    .await;
    driver.browser.close().await;
    result
}
