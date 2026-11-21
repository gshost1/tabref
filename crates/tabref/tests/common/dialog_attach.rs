use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde_json::json;
use tabref_cdp::{CdpConnection, PageSession, attach_to_target_with, connect, list_page_targets};
use tabref_core::{AppError, Deadline};

use super::common::{Harness, harness, live};

fn fresh(tabref: &Harness) -> (CdpConnection, String) {
    let deadline = Deadline::after(5000).unwrap();
    let mut connection = connect(&tabref.browser.endpoint().unwrap(), deadline).unwrap();
    let target = list_page_targets(&mut connection, deadline)
        .unwrap()
        .remove(0)
        .target_id;
    (connection, target)
}

fn attach(
    connection: &mut CdpConnection,
    target: &str,
    legacy: bool,
) -> Result<PageSession, AppError> {
    let deadline = Deadline::after(4000).unwrap();
    let calls = [
        ("Page.enable", json!({})),
        ("Page.setLifecycleEventsEnabled", json!({"enabled":true})),
    ];
    if !legacy {
        return attach_to_target_with(connection, target, &calls, deadline);
    }
    let result = connection.call(
        "Target.attachToTarget",
        json!({"targetId":target,"flatten":true}),
        None,
        deadline,
    )?;
    let session_id = result["sessionId"].as_str().unwrap().to_owned();
    let mut all = vec![("Runtime.runIfWaitingForDebugger", json!({}))];
    all.extend(calls);
    connection.call_all(&all, Some(&session_id), deadline)?;
    Ok(PageSession {
        target_id: target.to_owned(),
        session_id,
    })
}

fn timed(tabref: &Harness, legacy: bool) -> (Result<PageSession, AppError>, Duration) {
    let (mut connection, target) = fresh(tabref);
    let started = Instant::now();
    let result = attach(&mut connection, &target, legacy);
    (result, started.elapsed())
}

fn arm(tabref: &Harness, expression: &str) {
    let (mut connection, target) = fresh(tabref);
    let page = attach(&mut connection, &target, true).unwrap();
    connection
        .call(
            "Runtime.evaluate",
            json!({"expression":expression}),
            Some(&page.session_id),
            Deadline::after(4000).unwrap(),
        )
        .unwrap();
}

#[test]
fn cold_and_briefly_busy_attaches_succeed() {
    if !live() {
        return;
    }
    let launch = Instant::now();
    let tabref = harness("attach-cold");
    let (result, cold) = timed(&tabref, false);
    result.unwrap();
    eprintln!(
        "cold launch + first attach: {:?}; attach: {cold:?}",
        launch.elapsed()
    );
    arm(
        &tabref,
        "setTimeout(() => { const until = Date.now() + 140; while (Date.now() < until) {} }, 100)",
    );
    std::thread::sleep(Duration::from_millis(120));
    let (result, elapsed) = timed(&tabref, false);
    result.unwrap();
    eprintln!("briefly busy attach succeeded: {elapsed:?}");
}

#[test]
fn a_busy_renderer_gets_a_possibility_not_a_dialog_diagnosis() {
    if !live() {
        return;
    }
    let tabref = harness("attach-busy");
    arm(&tabref, "setTimeout(() => { while (true) {} }, 300)");
    std::thread::sleep(Duration::from_millis(600));
    let (result, elapsed) = timed(&tabref, false);
    let error = result.unwrap_err();
    eprintln!(
        "busy-loop attach: {elapsed:?}: {}",
        error.suggestion.as_deref().unwrap_or_default()
    );
    assert!(
        elapsed < Duration::from_millis(1500),
        "{elapsed:?}: {error:?}"
    );
    assert!(
        error
            .suggestion
            .unwrap_or_default()
            .contains("a dialog may be open or the page is busy")
    );
}

struct CpuLoad {
    stop: Arc<AtomicBool>,
    workers: Vec<std::thread::JoinHandle<()>>,
}

impl CpuLoad {
    fn start() -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let count = std::thread::available_parallelism().map_or(2, |count| count.get().min(8));
        let workers = (0..count)
            .map(|_| {
                let stop = Arc::clone(&stop);
                std::thread::spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        std::hint::spin_loop();
                    }
                })
            })
            .collect();
        eprintln!("CPU load: {count} busy workers");
        Self { stop, workers }
    }
}

impl Drop for CpuLoad {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for worker in self.workers.drain(..) {
            worker.join().unwrap();
        }
    }
}

fn percentile(values: &mut [Duration], percent: usize) -> Duration {
    values.sort();
    values[(values.len() - 1) * percent / 100]
}

/// Reproducible attach timings and an already-open-dialog recovery probe.
#[test]
#[ignore]
fn measure_attach_paths_and_retry_policy() {
    if !live() {
        return;
    }
    let tabref = harness("attach-measure");
    let mut old = Vec::new();
    let mut new = Vec::new();
    for round in 0..100 {
        for legacy in [round % 2 == 0, round % 2 != 0] {
            let (result, elapsed) = timed(&tabref, legacy);
            result.unwrap();
            if legacy {
                old.push(elapsed);
            } else {
                new.push(elapsed);
            }
        }
    }
    eprintln!(
        "common path (100 each): old median {:?} p95 {:?}; new median {:?} p95 {:?}",
        percentile(&mut old, 50),
        percentile(&mut old, 95),
        percentile(&mut new, 50),
        percentile(&mut new, 95)
    );
    arm(&tabref, "setTimeout(() => confirm('Already open?'), 300)");
    std::thread::sleep(Duration::from_millis(800));
    let (result, elapsed) = timed(&tabref, false);
    eprintln!("existing dialog attach: {elapsed:?}: {result:?}");
    assert!(result.is_err());
    let (mut connection, target) = fresh(&tabref);
    let attached = connection
        .call(
            "Target.attachToTarget",
            json!({"targetId":target,"flatten":true}),
            None,
            Deadline::after(4000).unwrap(),
        )
        .unwrap();
    let session = attached["sessionId"].as_str().unwrap();
    for accept in [true, false] {
        let answer = connection.call(
            "Page.handleJavaScriptDialog",
            json!({"accept":accept}),
            Some(session),
            Deadline::after(500).unwrap(),
        );
        eprintln!("already-open dialog answer accept={accept}: {answer:?}");
        assert!(answer.is_err());
    }
    drop(connection);
    drop(tabref);
    let _load = CpuLoad::start();
    std::thread::sleep(Duration::from_millis(200));
    let launch = Instant::now();
    let tabref = harness("attach-loaded");
    let (result, elapsed) = timed(&tabref, false);
    result.unwrap();
    eprintln!(
        "CPU-loaded cold launch + first attach {:?}; attach {elapsed:?}",
        launch.elapsed()
    );
    let mut loaded = Vec::new();
    for _ in 0..20 {
        let (result, elapsed) = timed(&tabref, false);
        result.unwrap();
        loaded.push(elapsed);
    }
    eprintln!(
        "CPU-loaded attach (20): median {:?} p95 {:?}",
        percentile(&mut loaded, 50),
        percentile(&mut loaded, 95)
    );
}
