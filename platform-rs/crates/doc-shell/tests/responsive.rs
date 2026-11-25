#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Catalog normalisation (parse, index, copy the rows: tens of milliseconds for the real catalog, much more for a
//! big one) must not run on a thread that has to stay responsive. The runtime here has ONE worker thread, so any
//! normalisation done inline would stall every other task, including this test's ticker, for its whole duration.

mod support;

use std::time::{Duration, Instant};

use doc_contract::Mode;
use doc_lookup::catalog::{parse_fixed, Query};
use support::*;

fn big_catalog(rows: usize) -> String {
    let all: Vec<String> = (0..rows).map(|i| row(&format!("https://content.example.org/o/l{}_{}_text_reg", i % 3000, i), "reg", &format!("l{}", i % 3000))).collect();
    catalog_body(&all)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn catalog_normalisation_runs_off_the_runtime_threads() {
    let body = big_catalog(120_000);
    // how long the normalisation takes by itself, inline (the figure a stall would show up as)
    let t = Instant::now();
    let parsed = parse_fixed(body.as_bytes(), Query::Primary);
    let parse_ms = t.elapsed().as_millis() as u64;
    assert!(parsed.is_ok());
    assert!(parse_ms >= 150, "the test needs a catalog whose normalisation is slow enough to be seen ({parse_ms} ms)");
    let env = start(vec![Step::Ok(body)], Knobs::default()).await;
    // a ticker on the same runtime: its longest gap is the longest the runtime thread was away
    let ticker = tokio::spawn(async {
        let mut max = Duration::ZERO;
        let mut last = Instant::now();
        for _ in 0..400 {
            tokio::time::sleep(Duration::from_millis(5)).await;
            max = max.max(last.elapsed());
            last = Instant::now();
        }
        max
    });
    let t = Instant::now();
    let (status, _, _) = env.get("/language_codes_and_names", Mode::Fixed).await;
    let took = t.elapsed();
    assert_eq!(status, 200);
    let max_gap = ticker.await.unwrap();
    println!("inline normalisation alone {parse_ms} ms; the request took {} ms; longest ticker gap {} ms", took.as_millis(), max_gap.as_millis());
    assert!(max_gap.as_millis() as u64 * 3 < parse_ms, "the runtime thread was blocked for {} ms by a {parse_ms} ms normalisation", max_gap.as_millis());
}
