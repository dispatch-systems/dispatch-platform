//! Paycom's live probes that need Paycom alone, ignored: an operator runs one at a time
//! against a copy of a DSP, with the `operator-probes` feature. Never run by CI or print
//! provider records. Each installs a registry of Paycom, as the app installs its own.
use super::*;
use crate::{collections::timecards::extract, connection::SEARCH};

fn concurrency_levels(value: &str) -> Result<Vec<usize>> {
    let levels: Vec<usize> = value
        .split(',')
        .map(|part| {
            part.trim()
                .parse::<usize>()
                .map_err(|_| Error::new("benchmark_configuration_required", 400))
        })
        .collect::<Result<_>>()?;
    ensure(
        !levels.is_empty() && levels.iter().all(|n| *n > 0),
        "benchmark_configuration_required",
        400,
    )?;
    Ok(levels)
}
/// Numbers as floats, so a record from here and one through CDP compare by value.
fn numbers(value: &Value) -> Value {
    match value {
        Value::Number(n) => json!(n.as_f64()),
        Value::Array(items) => Value::Array(items.iter().map(numbers).collect()),
        Value::Object(map) => {
            Value::Object(map.iter().map(|(k, v)| (k.clone(), numbers(v))).collect())
        }
        other => other.clone(),
    }
}
/// Where two records differ, as field paths without values or indexes.
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
/// `timecard.js` on a response the signed-in tab fetched: its record, or its error code.
async fn read_in_tab(
    driver: &Driver,
    source: &str,
    code: &str,
    period: &Value,
) -> Result<std::result::Result<Value, String>> {
    let config = json!({"employeeCode":code,"period":period,"sourceUrl":source});
    let extractor = include_str!("../../scripts/timecard.js")
        .trim()
        .trim_end_matches(';');
    driver.page.evaluate(&format!(r#"(()=>{{globalThis.dispatchParity=null;(async()=>{{try{{
        const response=await fetch({source},{{credentials:'include',redirect:'error',cache:'no-store',
            signal:AbortSignal.timeout(30000)}});
        if(response.status!==200)throw 'status_'+response.status;
        const reader=response.body.getReader(),decoder=new TextDecoder('utf-8',{{fatal:true}});let text='';
        for(;;){{const part=await reader.read();if(part.done)break;text+=decoder.decode(part.value,{{stream:true}});}}
        text+=decoder.decode();const document=new DOMParser().parseFromString(text,'text/html'),location={{href:{source}}};
        let record=null,error=null;
        try{{record=({extractor})({config});}}catch(e){{error=String(e&&e.message||'error');}}
        globalThis.dispatchParity={{ok:true,record,error}};
      }}catch(e){{globalThis.dispatchParity={{ok:false,error:typeof e==='string'?e:'fetch'}};}}}})();return true;}})()"#,
        source = json!(source))).await?;
    let deadline = Instant::now() + Duration::from_secs(40);
    let outcome = loop {
        ensure(Instant::now() < deadline, "provider_timeout", 504)?;
        let value = driver.page.evaluate("globalThis.dispatchParity").await?;
        if !value.is_null() {
            break value;
        }
        sleep(Duration::from_millis(100)).await;
    };
    driver
        .page
        .evaluate("delete globalThis.dispatchParity")
        .await?;
    Ok(if outcome["record"].is_object() {
        Ok(outcome["record"].clone())
    } else {
        Err(s(&outcome, "error").to_owned())
    })
}
/// The same response over HTTP, read here: its record or why not, and parse time.
async fn read_here(
    http: &dispatch_core::collection::browser::http::Http,
    source: &str,
    code: &str,
    period: &Value,
    referer: &str,
) -> (std::result::Result<Value, String>, u128) {
    let html = match http.page(source, referer).await {
        Ok(html) => html,
        Err(dispatch_core::collection::browser::http::Refusal::Unavailable) => {
            return (Err("unavailable".into()), 0);
        }
        Err(dispatch_core::collection::browser::http::Refusal::Unreadable(label)) => {
            return (Err(label.into()), 0);
        }
    };
    let started = Instant::now();
    let read = extract::timecard(
        &html,
        &extract::Source {
            employee: code,
            period,
            url: source,
        },
    );
    let ms = started.elapsed().as_millis();
    (
        read.map_err(|reason| match reason {
            extract::Unreadable::WrongEmployee => "wrong_employee".to_owned(),
            extract::Unreadable::Invalid(code) => code.to_owned(),
        }),
        ms,
    )
}

// Every employee's response read two ways at about the same moment: `timecard.js`
// in the signed-in tab and the reader in this process. Prints counts and differing
// field paths only, never values.
#[tokio::test]
#[ignore = "requires an explicitly selected DSP and authenticated provider profile"]
async fn http_extraction_parity() -> Result<()> {
    dispatch_core::testing::install(&[&crate::COLLECTOR], &[]);
    let dsp = env_path("DISPATCH_BENCHMARK_DSP")?;
    let profile = dsp.join("state/browsers/paycom-browseros");
    let runtime = browseros::Runtime::new(
        std::path::Path::new("/opt/dispatch-browseros/0.50.5/browseros"),
        std::path::Path::new("/usr/local/libexec/dispatch-dev/bwrap"),
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
            &format!("{}:paycom:2", dsp.file_name().unwrap().to_str().unwrap()),
            &std::fs::read_to_string(secrets.join("paycom.enc"))?,
        )?;
        let auth = driver.authenticate(credentials, false).await?;
        ensure(auth["type"] == "ready", "benchmark_verification_required", 409)?;
        driver.credentials = Value::Null;
        let timezone = std::env::var("DISPATCH_BENCHMARK_TIMEZONE")
            .map_err(|_| Error::new("benchmark_configuration_required", 400))?;
        let roster = driver.roster(&timezone, None, true).await?;
        let http = roster
            .http
            .ok_or_else(|| Error::new("benchmark_http_unavailable", 502))?;
        let referer = format!("{}{SEARCH}", driver.origin);
        let started = Instant::now();
        let (mut equal, mut projected_equal, mut both_failed) = (0, 0, 0);
        let mut mismatched = 0;
        let mut parse_ms = Vec::new();
        for (index, employee) in roster.employees.iter().enumerate() {
            let code = s(employee, "code");
            let source = collection::timecard_url(&driver.origin, employee, &roster.period, false);
            let mut outcome = None;
            // A punch can land between the two reads; read both again once.
            for _ in 0..2 {
                let (tab, (here, ms)) = tokio::join!(
                    read_in_tab(&driver, &source, code, &roster.period),
                    read_here(&http, &source, code, &roster.period, &referer)
                );
                parse_ms.push(ms);
                let tab = tab?;
                let same = match (&tab, &here) {
                    (Ok(a), Ok(b)) => numbers(a) == numbers(b),
                    (Err(a), Err(b)) => a == b,
                    _ => false,
                };
                outcome = Some((tab, here, same));
                if same {
                    break;
                }
            }
            let (tab, here, same) = outcome.unwrap();
            match (&tab, &here) {
                (Ok(a), Ok(b)) => {
                    let projected = collection::project(a, code).ok();
                    let mine = collection::project(b, code).ok();
                    if projected.is_some() && projected == mine {
                        projected_equal += 1;
                    }
                    if same {
                        equal += 1;
                    } else {
                        mismatched += 1;
                        let mut paths = std::collections::BTreeSet::new();
                        differences(&numbers(a), &numbers(b), "", &mut paths);
                        eprintln!(
                            "PARITY {}",
                            json!({"ordinal":index+1,"differs":paths,"projectedEqual":projected==mine})
                        );
                    }
                }
                _ if same => both_failed += 1,
                _ => {
                    mismatched += 1;
                    eprintln!(
                        "PARITY {}",
                        json!({"ordinal":index+1,"tab":tab.as_ref().err(),"here":here.as_ref().err()})
                    );
                }
            }
        }
        parse_ms.sort();
        eprintln!(
            "PARITY {}",
            json!({"employees":roster.employees.len(),"recordsEqual":equal,"projectedEqual":projected_equal,
                "bothRefused":both_failed,"mismatched":mismatched,"elapsedMs":started.elapsed().as_millis(),
                "parseP50Ms":parse_ms.get(parse_ms.len()/2),"parseMaxMs":parse_ms.last()})
        );
        ensure(mismatched == 0, "benchmark_parity_failed", 409)
    }
    .await;
    driver.browser.close().await;
    result
}

// Every employee's timecard over HTTP at each of several concurrency levels, with one
// session for all of them. Prints timings and refusal labels only. Stops raising the
// level as soon as Paycom throttles or fails a request.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires an explicitly selected DSP and authenticated provider profile"]
async fn http_concurrency() -> Result<()> {
    dispatch_core::testing::install(&[&crate::COLLECTOR], &[]);
    let levels = concurrency_levels(
        &std::env::var("DISPATCH_BENCHMARK_LEVELS").unwrap_or_else(|_| "2,4,6".into()),
    )?;
    let dsp = env_path("DISPATCH_BENCHMARK_DSP")?;
    let profile = dsp.join("state/browsers/paycom-browseros");
    let runtime = browseros::Runtime::new(
        std::path::Path::new("/opt/dispatch-browseros/0.50.5/browseros"),
        std::path::Path::new("/usr/local/libexec/dispatch-dev/bwrap"),
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
    let roster = async {
        let secrets = dsp.join("secrets");
        let credentials = dispatch_core::foundation::crypto::decrypt(
            &db::key_file(&secrets.join("vault.key"))?,
            &format!("{}:paycom:2", dsp.file_name().unwrap().to_str().unwrap()),
            &std::fs::read_to_string(secrets.join("paycom.enc"))?,
        )?;
        let auth = driver.authenticate(credentials, false).await?;
        ensure(
            auth["type"] == "ready",
            "benchmark_verification_required",
            409,
        )?;
        driver.credentials = Value::Null;
        let timezone = std::env::var("DISPATCH_BENCHMARK_TIMEZONE")
            .map_err(|_| Error::new("benchmark_configuration_required", 400))?;
        driver.roster(&timezone, None, true).await
    }
    .await;
    driver.browser.close().await;
    let roster = roster?;
    let http = Arc::new(
        roster
            .http
            .ok_or_else(|| Error::new("benchmark_http_unavailable", 502))?,
    );
    let employees = Arc::new(roster.employees);
    let period = Arc::new(roster.period);
    let origin = driver.origin.clone();
    for (step, lanes) in levels.into_iter().enumerate() {
        if step > 0 {
            sleep(Duration::from_secs(15)).await;
        }
        let next = Arc::new(AtomicU64::new(0));
        let stopped = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let started = Instant::now();
        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..lanes {
            let (http, employees, period, next, origin, stopped) = (
                http.clone(),
                employees.clone(),
                period.clone(),
                next.clone(),
                origin.clone(),
                stopped.clone(),
            );
            tasks.spawn(async move {
                let mut reads = Vec::new();
                loop {
                    if stopped.load(Ordering::SeqCst) {
                        break;
                    }
                    let index = next.fetch_add(1, Ordering::SeqCst) as usize;
                    let Some(employee) = employees.get(index) else {
                        break;
                    };
                    let source = collection::timecard_url(&origin, employee, &period, false);
                    let fetched = Instant::now();
                    let page = http.page(&source, &format!("{origin}{SEARCH}")).await;
                    let ms = fetched.elapsed().as_millis();
                    let outcome = match page {
                        Err(dispatch_core::collection::browser::http::Refusal::Unavailable) => {
                            Err("unavailable".to_owned())
                        }
                        Err(dispatch_core::collection::browser::http::Refusal::Unreadable(
                            label,
                        )) => Err(label.to_owned()),
                        Ok(html) => {
                            let (code, period) = (s(employee, "code").to_owned(), period.clone());
                            tokio::task::spawn_blocking(move || {
                                extract::timecard(
                                    &html,
                                    &extract::Source {
                                        employee: &code,
                                        period: &period,
                                        url: &source,
                                    },
                                )
                                .map(|_| ())
                                .map_err(|reason| format!("{reason:?}"))
                            })
                            .await
                            .unwrap_or_else(|_| Err("extraction_stopped".into()))
                        }
                    };
                    if outcome.is_err() {
                        stopped.store(true, Ordering::SeqCst);
                    }
                    reads.push((ms, outcome));
                }
                reads
            });
        }
        let mut reads = Vec::new();
        while let Some(lane) = tasks.join_next().await {
            reads.extend(lane.unwrap());
        }
        let elapsed = started.elapsed().as_millis();
        let mut times = reads.iter().map(|(ms, _)| *ms).collect::<Vec<_>>();
        times.sort();
        let mut refusals = std::collections::BTreeMap::<String, usize>::new();
        for (_, outcome) in &reads {
            if let Err(label) = outcome {
                *refusals.entry(label.clone()).or_default() += 1;
            }
        }
        let valid = reads.iter().filter(|(_, o)| o.is_ok()).count();
        eprintln!(
            "LEVEL {}",
            json!({"lanes":lanes,"reads":reads.len(),"valid":valid,"elapsedMs":elapsed,
                "p50Ms":times.get(times.len()/2),"p90Ms":times.get(times.len()*9/10),
                "maxMs":times.last(),"refusals":refusals})
        );
        if !refusals.is_empty() {
            eprintln!("LEVEL {}", json!({"stopped":"request_refused"}));
            break;
        }
    }
    Ok(())
}
