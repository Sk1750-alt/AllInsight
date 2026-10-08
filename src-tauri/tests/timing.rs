//! Timing probe for cleanup discovery.
//!
//! Not a correctness test: it prints how long each category takes so a slow
//! one can be found rather than guessed at. Run with:
//!
//!   cargo test --test timing -- --nocapture --ignored

use std::sync::atomic::AtomicBool;
use std::time::Instant;

use allinsight_lib::services::cleanup::{self, ALL_CLEANUP_CATEGORIES};
use allinsight_lib::services::security::ProtectedPaths;

#[test]
#[ignore]
fn time_each_cleanup_category() {
    let engine = ProtectedPaths::new(&[]);
    let cancelled = AtomicBool::new(false);

    for category in ALL_CLEANUP_CATEGORIES {
        let started = Instant::now();
        let (preview, scan) = cleanup::discover(&engine, Some(&[category]), &cancelled);
        let elapsed = started.elapsed();
        let report = preview.categories.iter().find(|c| c.category == category);
        println!(
            "{:>32?}  {:>8.2}s  {:>10} bytes  {:>7} items  {:>7} candidates",
            category,
            elapsed.as_secs_f64(),
            report.map(|r| r.bytes).unwrap_or(0),
            report.map(|r| r.items).unwrap_or(0),
            scan.order.len()
        );
    }
}
