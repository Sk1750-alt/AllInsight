//! Timing probe for the calls the interface makes repeatedly.
//!
//! Not a correctness test. It reports how long each service call takes so a
//! stall can be attributed rather than guessed at. Run with:
//!
//!   cargo test --test perf -- --nocapture --ignored

use std::time::Instant;

use allinsight_lib::services::system::SystemMonitor;

fn time<T>(label: &str,times: u32, mut f: impl FnMut() -> T) {
    // Warm once so first-call caching does not distort the figure.
    let _ = f();
    let started = Instant::now();
    for _ in 0..times {
        let _ = f();
    }
    let per = started.elapsed().as_secs_f64() * 1000.0 / times as f64;
    let flag = if per > 100.0 {
        "  <-- BLOCKS THE UI"
    } else if per > 33.0 {
        "  <-- drops frames"
    } else {
        ""
    };
    println!("{label:<46} {per:>8.1} ms{flag}");
}

#[test]
#[ignore]
fn time_the_calls_the_interface_polls() {
    let monitor = SystemMonitor::new();

    println!("\n--- polled continuously while a screen is open ---");
    time("monitor.sample()  [Overview 2s, Perf 1.5s]", 5, || {
        monitor.sample()
    });
    time("monitor.history()  [Perf 3s]", 5, || monitor.history());
    time("process::list(140, publishers)  [2.5s]", 3, || {
        allinsight_lib::services::process::list(&monitor, 140, true)
    });
    time("process::list(140, no publishers)", 3, || {
        allinsight_lib::services::process::list(&monitor, 140, false)
    });
    time("storage::overview()  [banner 60s]", 5, || {
        allinsight_lib::services::storage::overview()
    });

    println!("\n--- run on screen open ---");
    time("health::report()", 3, || {
        allinsight_lib::services::health::report()
    });
    time("battery::status()", 3, || {
        allinsight_lib::services::battery::status()
    });
    time("startup::list()", 3, || allinsight_lib::services::startup::list());
    time("apps::list(measure = false)", 2, || {
        allinsight_lib::services::apps::list(false)
    });
    println!();
}
