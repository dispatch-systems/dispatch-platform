//! Operator-only measurements of Paycom. Never run by CI or print provider records.
//! A probe that compares a collection with what the feature keeping it published needs
//! the app's registry, so the app's tests run it. The probes that need Paycom alone are
//! its ignored tests.
use crate::{
    collections::timecards::{collect as collection, projection},
    connection::{Driver, rest},
};
use dispatch_core::{
    Error, Result,
    collection::browser::{browseros, page::call},
    db::{self, s},
    ensure,
};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::time::{Instant, sleep};

fn env_path(name: &str) -> Result<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .ok_or_else(|| Error::new("benchmark_configuration_required", 400))
}
fn memory_tree(root: u32) -> [u64; 4] {
    let mut processes = Vec::new();
    for entry in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        let Ok(status) = std::fs::read_to_string(entry.path().join("status")) else {
            continue;
        };
        let field = |prefix: &str| {
            status
                .lines()
                .find_map(|l| l.strip_prefix(prefix))
                .and_then(|s| s.split_whitespace().next())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0)
        };
        processes.push((pid, field("PPid:") as u32, field("VmRSS:")));
    }
    let mut ids = std::collections::HashSet::from([root]);
    loop {
        let before = ids.len();
        for &(pid, parent, _) in &processes {
            if ids.contains(&parent) {
                ids.insert(pid);
            }
        }
        if ids.len() == before {
            break;
        }
    }
    let mut totals = [0; 4];
    for (pid, _, rss) in processes.iter().filter(|(pid, _, _)| ids.contains(pid)) {
        totals[0] += rss;
        match std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup")) {
            Ok(smaps) => {
                for line in smaps.lines() {
                    let Some((name, value)) = line.split_once(':') else {
                        continue;
                    };
                    let kib = value
                        .split_whitespace()
                        .next()
                        .and_then(|v| v.parse::<u64>().ok())
                        .unwrap_or(0);
                    match name {
                        "Pss" => totals[1] += kib,
                        "Private_Clean" | "Private_Dirty" | "Private_Hugetlb" => totals[2] += kib,
                        _ => (),
                    }
                }
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    || error.raw_os_error() == Some(libc::ESRCH) => {}
            Err(_) => totals[3] += 1,
        }
    }
    totals
}
/// A live collection of the whole roster, timed and sampled for memory, compared card by
/// card with what the feature keeping timecards last published.
pub async fn measure_live_collection() -> Result<()> {
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
    let peak = Arc::new(std::array::from_fn::<_, 4, _>(|_| AtomicU64::new(0)));
    let counter = peak.clone();
    let pid = driver.browser.process_id();
    // When the browser was last seen running, in ms after sampling began.
    let began = Instant::now();
    let alive = Arc::new(AtomicU64::new(0));
    let seen = alive.clone();
    let sampler = tokio::spawn(async move {
        loop {
            let tree = memory_tree(pid);
            if tree[0] > 0 {
                seen.store(began.elapsed().as_millis() as u64, Ordering::Relaxed);
            }
            for (counter, value) in counter.iter().zip(tree) {
                counter.fetch_max(value, Ordering::Relaxed);
            }
            sleep(Duration::from_millis(250)).await;
        }
    });
    let result = async {
        let secrets = dsp.join("secrets");
        let credentials = dispatch_core::foundation::crypto::decrypt(&db::key_file(&secrets.join("vault.key"))?,
            &format!("{}:paycom:2",dsp.file_name().unwrap().to_str().unwrap()), &std::fs::read_to_string(secrets.join("paycom.enc"))?)?;
        let auth=driver.authenticate(credentials,false).await?;
        ensure(auth["type"]=="ready","benchmark_verification_required",409)?;
        driver.credentials=Value::Null;
        let timezone=std::env::var("DISPATCH_BENCHMARK_TIMEZONE").map_err(|_|Error::new("benchmark_configuration_required",400))?;
        let started=Instant::now();
        let recorder=dispatch_core::collection::metrics::Recorder::new(&json!({}));
        let data=driver.collect(&timezone, collection::Selection::Date(None), &recorder, None, |progress,_| async move {
            if progress % 10 == 0 { eprintln!("BENCH {}",json!({"progress":progress})); }
            Ok(())
        }, std::env::var("DISPATCH_BENCHMARK_HTTP").is_ok_and(|v| v == "1")).await?;
        let elapsed=started.elapsed().as_millis();
        let opened=started.duration_since(began).as_millis() as u64;
        eprintln!("BENCH {}",json!({"browserMsDuringCollection":alive.load(Ordering::Relaxed).saturating_sub(opened)}));
        let reads=serde_json::to_value(recorder.snapshot())?["pageReads"].clone();
        eprintln!("BENCH {}",json!({"completedReads":reads["completed"],"directReads":reads["direct"],
            "spotChecked":reads["spotChecked"],"pageRetries":reads["retries"],"failedReads":reads["failures"].as_array().map(Vec::len)}));
        let collection_peak=peak.each_ref().map(|value| value.load(Ordering::Relaxed));
        let database=db::Db(rusqlite::Connection::open_with_flags(dispatch_core::collection::registry::database_path(&dsp,
            crate::PROVIDER)?,rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?);
        let mut expected=std::collections::BTreeMap::new();
        // Timecard keeps what the last collection published.
        for card in dispatch_core::manifest::registry().keeper(crate::timecards::JOB_KIND)
            .published(&database)? {
            expected.insert((s(&card,"employeeCode").to_owned(),s(&card,"date").to_owned()),card);
        }
        let records=data["timecards"].as_array().unwrap();
        let mut equal=0; let mut changed=0; let mut added=0;
        let mut differences=std::collections::BTreeSet::new();
        for card in records {
            match expected.get(&(s(card,"employeeCode").to_owned(),s(card,"date").to_owned())) {
                Some(old) if old==card => equal+=1,
                Some(_) => {changed+=1; differences.insert(s(card,"employeeCode").to_owned());},
                None => added+=1,
            }
        }
        eprintln!("BENCH {}",json!({"collectionMs":elapsed,
            "employeeCount":data["employees"].as_array().unwrap().len(),"timecardCount":records.len(),
            "comparedEqual":equal,"changedSincePublication":changed,"addedSincePublication":added,
            "previousCardCount":expected.len(),"collectionPeakRssKiB":collection_peak[0],
            "collectionPeakPssKiB":collection_peak[1],"collectionPeakPrivateKiB":collection_peak[2],"unreadableSmaps":collection_peak[3]}));

        // "1" keeps the original six-page probe; a larger number (or "all") covers
        // the roster so parity is measured rather than sampled.
        if let Ok(value) = std::env::var("DISPATCH_BENCHMARK_RESPONSE") {
            let samples = match value.as_str() {
                "1" => 6,
                "all" => usize::MAX,
                other => other.parse().map_err(|_| Error::new("benchmark_configuration_required", 400))?,
            };
            inspect_responses(&driver, &data, samples).await?;
        }

        if !differences.is_empty() {
            driver.new_page().await?;
            let from=s(&data,"from"); let to=s(&data,"to");
            let day=chrono::NaiveDate::parse_from_str(from,"%Y-%m-%d").map_err(|_|Error::new("invalid_period",409))?;
            let period=json!({"start":from,"end":to,"key":format!("{from}_{to}"),
                "dates":(0..14).map(|i|(day+chrono::Duration::days(i)).to_string()).collect::<Vec<_>>()});
            let mut confirmed=0;
            for code in &differences {
                let reference=reference_timecard(&driver,code,&period).await?;
                let actual=records.iter().filter(|card|s(card,"employeeCode")==code).cloned().collect::<Vec<_>>();
                ensure(actual==reference,"benchmark_reference_mismatch",409)?;
                confirmed+=1;
            }
            eprintln!("BENCH {}",json!({"changedEmployeesConfirmedBySequentialReads":confirmed}));
        }
        Ok(())
    }.await;
    sampler.abort();
    let exit = driver.browser.close().await;
    eprintln!(
        "BENCH {}",
        json!({"peakRssKiB":peak[0].load(Ordering::Relaxed),"peakPssKiB":peak[1].load(Ordering::Relaxed),
            "peakPrivateKiB":peak[2].load(Ordering::Relaxed),"unreadableSmaps":peak[3].load(Ordering::Relaxed),
            "supervisorReaped":exit.supervisor_reaped})
    );
    result
}

// Preserve the original serialized navigation/read path as an independent live
// reference. This exists only in the ignored operator benchmark, never runtime.
async fn reference_timecard(driver: &Driver, code: &str, period: &Value) -> Result<Vec<Value>> {
    let path = format!(
        "/v4/cl/web.php/timecard/index?firstrefno={code}&perioddates={}&formtype=SUMMARY&dispatch_timecards=1",
        s(period, "key")
    );
    let source = format!("{}{path}", driver.origin);
    driver.navigate(&path).await?;
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        ensure(Instant::now() < deadline, "provider_timeout", 504)?;
        let frame = driver.page.frame().await?;
        if s(&frame, "url") == source
            && driver
                .page
                .evaluate(
                    "document.readyState==='complete'&&!!document.query\
                Selector('#tbltimesheet')&&!!document.querySelector('#periodtotals')",
                )
                .await?
                == true
        {
            break;
        }
        ensure(
            driver.page.trusted(s(&frame, "url")),
            "authentication_failed",
            409,
        )?;
        sleep(Duration::from_millis(200)).await;
    }
    let config = json!({"employeeCode":code,"period":period,"sourceUrl":source});
    let record = driver
        .page
        .evaluate(&call(include_str!("../scripts/timecard.js"), &config))
        .await?;
    projection::project(&record, code)
}

// An explicit, read-only experiment. Raw HTML never leaves the authenticated
// browser; only validated records and aggregate structure reach this process.
// The detached document runs no provider scripts and is never inserted in a tab.
async fn inspect_responses(driver: &Driver, data: &Value, samples: usize) -> Result<()> {
    let from = s(data, "from");
    let to = s(data, "to");
    let day = chrono::NaiveDate::parse_from_str(from, "%Y-%m-%d")
        .map_err(|_| Error::new("invalid_period", 409))?;
    let period = json!({"start":from,"end":to,"key":format!("{from}_{to}"),
        "dates":(0..14).map(|i|(day+chrono::Duration::days(i)).to_string()).collect::<Vec<_>>()});
    let mut equal = 0;
    let mut validated = 0;
    let mut rejected = Vec::new();
    let started = Instant::now();
    for (index, employee) in data["employees"]
        .as_array()
        .unwrap()
        .iter()
        .take(samples)
        .enumerate()
    {
        let code = s(employee, "code");
        let fetched = Instant::now();
        let source = format!(
            "{}/v4/cl/web.php/timecard/index?firstrefno={code}&perioddates={}&formtype=SUMMARY&dispatch_timecards=1",
            driver.origin,
            s(&period, "key")
        );
        let config = json!({"employeeCode":code,"period":period,"sourceUrl":source});
        let extractor = include_str!("../scripts/timecard.js")
            .trim()
            .trim_end_matches(';');
        driver.page.evaluate(&format!(r#"(()=>{{globalThis.dispatchProbe=null;(async()=>{{try{{
          let stage='fetch';
          const response=await fetch({source},{{credentials:'include',redirect:'error',cache:'no-store',
              signal:AbortSignal.timeout(30000)}});
          if(response.status!==200)throw 'status_'+response.status;
          if(!/^text\/html(?:;|$)/i.test(response.headers.get('content-type')||'')||!response.body)throw 'content_type';
          const reader=response.body.getReader(),decoder=new TextDecoder('utf-8',{{fatal:true}});let text='',size=0;
          for(;;){{const part=await reader.read();if(part.done)break;size+=part.value.byteLength;if(size>2097152){{await reader.cancel();throw 'size';}}text+=decoder.decode(part.value,{{stream:true}});}}
          text+=decoder.decode();const document=new DOMParser().parseFromString(text,'text/html'),location={{href:{source}}};
          const result={{bytes:size,tablePresent:!!document.querySelector('#tbltimesheet'),
              rows:document.querySelectorAll('#tbltimesheet > tbody > tr').length,scripts:document.scripts.length,record:null}};
          try{{result.record=({extractor})({config});}}catch{{}}
          globalThis.dispatchProbe={{ok:true,value:result}};
        }}catch(error){{globalThis.dispatchProbe={{ok:false,reason:typeof error==='string'?error:String(error&&error.name||'error')}};}}}})();return true;}})()"#,
                 source=json!(source))).await?;
        let deadline = Instant::now() + Duration::from_secs(35);
        let probe = loop {
            ensure(Instant::now() < deadline, "provider_timeout", 504)?;
            let probe = driver.page.evaluate("globalThis.dispatchProbe").await?;
            if !probe.is_null() {
                break probe;
            }
            sleep(Duration::from_millis(100)).await;
        };
        driver
            .page
            .evaluate("delete globalThis.dispatchProbe")
            .await?;
        if probe["ok"] != true {
            eprintln!(
                "RESPONSE {}",
                json!({"ordinal":index+1,"ms":fetched.elapsed().as_millis(),"fetchFailed":probe["reason"]})
            );
            continue;
        }
        let value = &probe["value"];
        let actual = projection::project(&value["record"], code);
        let reference = data["timecards"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|card| s(card, "employeeCode") == code)
            .cloned()
            .collect::<Vec<_>>();
        if actual.is_ok() {
            validated += 1;
        }
        // Field names only: which parts of a card differ, never their values.
        let mut differs = std::collections::BTreeSet::new();
        if let Ok(cards) = &actual {
            for (card, expected) in cards.iter().zip(&reference) {
                for key in ["date", "hours", "status"] {
                    if card[key] != expected[key] {
                        differs.insert(key.to_owned());
                    }
                }
                let (a, b) = (card["punches"].as_array(), expected["punches"].as_array());
                match (a, b) {
                    (Some(a), Some(b)) if a.len() == b.len() => {
                        for (x, y) in a.iter().zip(b) {
                            for (field, value) in x.as_object().into_iter().flatten() {
                                if y[field] != *value {
                                    differs.insert(format!("punches.{field}"));
                                }
                            }
                        }
                    }
                    _ => {
                        differs.insert("punches.length".to_owned());
                    }
                }
            }
            if cards.len() != reference.len() {
                differs.insert("cards.length".to_owned());
            }
        }
        let failure = actual.as_ref().err().map(|error| error.code.clone());
        let matches = actual.is_ok_and(|cards| cards == reference);
        if matches {
            equal += 1;
        } else if rejected.len() < 4 {
            rejected.push(code.to_owned());
        }
        eprintln!(
            "RESPONSE {}",
            json!({"ordinal":index+1,"ms":fetched.elapsed().as_millis(),"bytes":value["bytes"],
                "tablePresent":value["tablePresent"],"rows":value["rows"],"scripts":value["scripts"],
                "validated":!value["record"].is_null(),"matchesRendered":matches,"differs":differs,"error":failure})
        );
        driver.page.collect_garbage().await?;
    }
    eprintln!(
        "RESPONSE {}",
        json!({"samples":samples.min(data["employees"].as_array().unwrap().len()),"validated":validated,
            "equal":equal,"elapsedMs":started.elapsed().as_millis()})
    );
    // Why detached extraction was rejected: on the rendered page, tally each punch
    // cell child's static signature against whether layout shows it. No values.
    for code in rejected {
        let path = format!(
            "/v4/cl/web.php/timecard/index?firstrefno={code}&perioddates={}&formtype=SUMMARY&dispatch_timecards=1",
            s(&period, "key")
        );
        driver.navigate(&path).await?;
        let deadline = Instant::now() + Duration::from_secs(60);
        while Instant::now() < deadline
            && driver
                .page
                .evaluate(
                    "document.readyState==='complete'&&!!document.querySelector('#tbltimesheet\
                ')&&!!document.querySelector('#periodtotals')",
                )
                .await?
                != true
        {
            sleep(Duration::from_millis(200)).await;
        }
        let shape = driver.page.evaluate(r#"(()=>{const t=document.querySelector('#tbltimesheet');if(!t)return null;
          const heads=Array.from(t.querySelectorAll('thead [data-column]')).map(e=>e.getAttribute('data-column'));
          const time=/^(0?[1-9]|1[0-2]):[0-5][0-9] [AP]M$/,tally={};
          for(const row of t.querySelectorAll(':scope > tbody > tr'))for(const slot of ['i1','o1','i2',
              'o2']){const cell=row.children[heads.indexOf(slot)];if(!cell)continue;
            const kids=Array.from(cell.children);if(kids.length<2)continue;
            for(const kid of kids){const text=(kid.textContent||'').replace(/\s+/g,' ').trim();
              const key=[kid.tagName.toLowerCase(),'.'+Array.from(kid.classList).sort().join('.'),
                  kid.getAttribute('style')?'[style='+kid.getAttribute('style').replace(/\s+/g,'')+']':'',
                  kid.hidden?'[hidden]':'',time.test(text)?'<time>':text?'<text>':'<empty>',
                  (kid.offsetParent!==null&&kid.getClientRects().length>0)?'VISIBLE':'hidden'].join(' ');
              tally[key]=(tally[key]||0)+1;}}
          return tally;})()"#).await?;
        eprintln!("SHAPE {}", json!({"rendered":shape}));
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/backend/probes.rs"]
mod tests;

/// A field's name as a report may print it: a word of letters, with at most a short number
/// after it, that isn't the four characters of an employee's code. An answer keyed by
/// anything else, as by its records' ids, has its keys hidden as `pattern` hides a value.
fn field(name: &str) -> String {
    let word = name.trim_end_matches(|c: char| c.is_ascii_digit());
    let named = !word.is_empty()
        && name.len() <= 48
        && name.len() - word.len() <= 3
        && word.chars().all(|c| c.is_ascii_alphabetic() || c == '_')
        && !(name.len() == 4 && word.len() < 4);
    if named {
        name.to_owned()
    } else {
        pattern(&json!(name))
    }
}
/// What kind of value this is.
fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
/// The names of a record's fields and the kind of value each holds, never a value.
fn shape(record: &Value) -> Value {
    match record {
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(name, value)| (field(name), json!(kind(value))))
                .collect(),
        ),
        other => json!(kind(other)),
    }
}
/// How a value is written, its digits and letters hidden: `9999-99-99 99:99:99` for a time.
fn pattern(value: &Value) -> String {
    let text = match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    text.chars()
        .take(40)
        .map(|c| match c {
            c if c.is_ascii_digit() => '9',
            c if c.is_alphabetic() => 'A',
            c => c,
        })
        .collect()
}
/// What one function of Paycom's API answers, by its shape alone: its status, how its answer
/// is wrapped, and its first record's fields. For punches, how their times are written and
/// which kinds they come in, as the cards are made from those.
async fn answer_shape(client: &rest::Client, function: &str, path: &str) -> Result<Vec<Value>> {
    let answer = client.answer(path).await?;
    let parsed: Value = serde_json::from_str(&answer.body).unwrap_or(Value::Null);
    let rows = rest::rows(&answer.body).unwrap_or_default();
    let mut report = json!({"function":function,"status":answer.status,"bytes":answer.body.len(),
        "wrapper":shape(&parsed),"rows":rows.len(),"record":rows.first().map(shape)});
    if function == "punch_history" {
        let distinct = |field: &str, written: fn(&Value) -> String| {
            rows.iter()
                .map(|row| written(&row[field]))
                .collect::<std::collections::BTreeSet<_>>()
        };
        // A kind's code as Paycom writes one, one to three capital letters, a flag or a
        // single digit: anything else is named by what it is, never printed.
        let plain = |value: &Value| match value {
            Value::String(text)
                if (1..=3).contains(&text.len())
                    && text.bytes().all(|b| b.is_ascii_uppercase())
                    || text.len() == 1 && text.as_bytes()[0].is_ascii_digit() =>
            {
                text.clone()
            }
            Value::Number(number) if number.as_u64().is_some_and(|digit| digit <= 9) => {
                number.to_string()
            }
            Value::Bool(flag) => flag.to_string(),
            other => format!("another {}", kind(other)),
        };
        report["punchtime"] = json!(distinct("punchtime", pattern));
        report["timeadded"] = json!(distinct("timeadded", pattern));
        report["punchtype"] = json!(distinct("punchtype", plain));
        report["entrytype"] = json!(distinct("entrytype", plain));
        report["isdeleted"] = json!(distinct("isdeleted", plain));
    }
    eprintln!("SHAPE {report}");
    Ok(rows)
}

/// A collection through Paycom's API of the period the feature keeping timecards last
/// published, compared with that publication card by card: what shows whether hours worked
/// out from the API's punches are the hours Paycom's own pages total. Run it straight after
/// a collection by sign-in, so the two read the same punches. It needs the DSP's folder
/// (`DISPATCH_BENCHMARK_DSP`), where the DSP is (`DISPATCH_BENCHMARK_TIMEZONE`), and the SID
/// and Token (`DISPATCH_BENCHMARK_PAYCOM_SID`, `DISPATCH_BENCHMARK_PAYCOM_TOKEN`), which
/// it keeps nowhere. It prints each check, each function's answer by its shape, and counts:
/// never a name, a code, a time or an hour of anyone's.
pub async fn compare_api_collection() -> Result<()> {
    use crate::timecards::{EmployeeTimecardPeriod, PERIOD_DAYS, api};
    use std::collections::{BTreeMap, BTreeSet};
    let dsp = env_path("DISPATCH_BENCHMARK_DSP")?;
    let variable = |name: &str| {
        std::env::var(name).map_err(|_| Error::new("benchmark_configuration_required", 400))
    };
    let zone: chrono_tz::Tz = variable("DISPATCH_BENCHMARK_TIMEZONE")?
        .parse()
        .map_err(|_| Error::new("invalid_timezone", 400))?;
    let credentials = json!({"sid":variable("DISPATCH_BENCHMARK_PAYCOM_SID")?,
        "token":variable("DISPATCH_BENCHMARK_PAYCOM_TOKEN")?});
    let database = db::Db(rusqlite::Connection::open_with_flags(
        dispatch_core::collection::registry::database_path(&dsp, crate::PROVIDER)?,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?);
    // Timecard keeps what the last collection published.
    let mut published = BTreeMap::new();
    for card in dispatch_core::manifest::registry()
        .keeper(crate::timecards::JOB_KIND)
        .published(&database)?
    {
        published.insert(
            (
                s(&card, "employeeCode").to_owned(),
                s(&card, "date").to_owned(),
            ),
            card,
        );
    }
    let from: chrono::NaiveDate = published
        .keys()
        .map(|(_, date)| date.as_str())
        .min()
        .ok_or_else(|| Error::new("benchmark_publication_required", 409))?
        .parse()
        .map_err(|_| Error::new("invalid_period", 409))?;
    let period = EmployeeTimecardPeriod {
        from: from.to_string(),
        to: (from + chrono::Duration::days(PERIOD_DAYS - 1)).to_string(),
    };

    // Each function's answer by its shape, from whichever data center takes the SID and
    // Token, whether or not a collection can read it yet.
    let parsed = rest::Credentials::parse(&credentials)?;
    for center in rest::DATA_CENTERS {
        let client = rest::Client::new(center, &parsed)?;
        let listed = answer_shape(
            &client,
            "employee_directory",
            "api/v1/employeedirectory?pagesize=1",
        )
        .await?;
        let Some(code) = listed
            .first()
            .map(|row| s(row, "eecode"))
            .filter(|code| api::path_code(code))
        else {
            continue;
        };
        let (start, end) = api::window(zone, &period)?;
        answer_shape(&client, "employee", &format!("api/v1/employee/{code}")).await?;
        answer_shape(
            &client,
            "punch_history",
            &api::punch_history(code, start, end),
        )
        .await?;
        answer_shape(&client, "labor_allocation_categories", "api/v1/cl/category").await?;
        break;
    }

    let mut driver = rest::Driver::live(rest::Credentials::parse(&credentials)?, from);
    let connected = driver.connect().await;
    for check in driver.asked() {
        eprintln!(
            "CHECK {}",
            json!({"id":check.id,"state":check.state.as_str(),"error":check.error})
        );
    }
    connected?;
    let source = driver
        .source()
        .ok_or_else(|| Error::new("verification_required", 409))?;
    // The publication holds people the active directory may leave out by now, as a
    // collection by sign-in reads everyone known: they are read here too, by their code alone.
    let known = published
        .keys()
        .map(|(code, _)| code.as_str())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|code| {
            json!({"code":code,"name":code,"department":"","position":"","station":"",
                "active":false})
        })
        .collect::<Vec<_>>();
    let started = Instant::now();
    let data = api::everyone(
        &source,
        zone,
        &period,
        Some(&known),
        |done, total| async move {
            if done % 20 == 0 || done == total {
                eprintln!("BENCH {}", json!({"read":done,"of":total}));
            }
            Ok(())
        },
    )
    .await?;
    let cards = data["timecards"].as_array().unwrap();

    // How the two differ, counted: a card's hours, its status, its punches' times, or only
    // their kinds; and by how much the hours differ, in the hundredths both are kept to.
    let mut tally: BTreeMap<&str, usize> = BTreeMap::new();
    let mut by: BTreeMap<String, usize> = BTreeMap::new();
    let times = |card: &Value| {
        card["punches"]
            .as_array()
            .map(|punches| {
                punches
                    .iter()
                    .map(|punch| (punch["in"].clone(), punch["out"].clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    let mut read = BTreeSet::new();
    for card in cards {
        let key = (
            s(card, "employeeCode").to_owned(),
            s(card, "date").to_owned(),
        );
        let Some(pages) = published.get(&key) else {
            *tally.entry("cardsOnlyFromApi").or_default() += 1;
            continue;
        };
        read.insert(key);
        let hundredths =
            |card: &Value| (card["hours"].as_f64().unwrap_or(f64::NAN) * 100.).round() as i64;
        let difference = hundredths(card) - hundredths(pages);
        let same_hours = difference == 0;
        let same_status = card["status"] == pages["status"];
        let same_times = times(card) == times(pages);
        let same_punches = card["punches"] == pages["punches"];
        if !same_hours {
            *tally.entry("hoursDiffer").or_default() += 1;
            *by.entry(format!("{:+.2}", difference as f64 / 100.))
                .or_default() += 1;
        }
        if !same_status {
            *tally.entry("statusDiffers").or_default() += 1;
        }
        if !same_times {
            *tally.entry("punchTimesDiffer").or_default() += 1;
        } else if !same_punches {
            *tally.entry("punchKindsDiffer").or_default() += 1;
        }
        if same_hours && same_status && same_punches {
            *tally.entry("cardsEqual").or_default() += 1;
        }
    }
    let listed: BTreeSet<&str> = cards.iter().map(|card| s(card, "employeeCode")).collect();
    let known: BTreeSet<&str> = published.keys().map(|(code, _)| code.as_str()).collect();
    eprintln!(
        "BENCH {}",
        json!({"collectionMs":started.elapsed().as_millis(),"from":period.from,"to":period.to,
            "cardsFromApi":cards.len(),"cardsPublished":published.len(),
            "cardsOnlyPublished":published.len() - read.len(),"compared":tally,
            "hoursApiMinusPages":by,"employeesFromApi":listed.len(),"employeesPublished":known.len(),
            "employeesOnlyFromApi":listed.difference(&known).count(),
            "employeesOnlyPublished":known.difference(&listed).count()})
    );
    Ok(())
}
