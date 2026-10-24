//! `cargo run -p omcp-sim --release -- [seed] [scenario]` prints the SY1–SY6
//! report (scenario: `all` (default), `sy1`, `sy2`, … `sy6`).

use std::time::Instant;

use omcp_sim::{sy1, sy23, sy4, sy5, sy6};

fn main() {
    let mut args = std::env::args().skip(1);
    let seed: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(0x5EED);
    let which = args.next().unwrap_or_else(|| "all".into());
    let on = |s: &str| which == "all" || which == s;
    println!("seed = {seed:#x}");
    if on("sy1") {
        for (n, r) in [(1_000, 1), (10_000, 1), (10_000, 2)] {
            let t = Instant::now();
            let mut p = sy1::Sy1Params::new(n, seed);
            p.radio_radius = r;
            let rep = sy1::run(&p);
            println!(
                "SY1 n={n} radius={r}: {rep:?} ({} ms)",
                t.elapsed().as_millis()
            );
        }
    }
    if on("sy2") {
        println!("SY2: {:?}", sy23::sy2(100_000, seed));
    }
    if on("sy3") {
        println!("SY3: {:?}", sy23::sy3(55, 500, 10, seed));
    }
    if on("sy4") {
        for s in [
            sy4::Strategy::Flood,
            sy4::Strategy::Sync22,
            sy4::Strategy::Sync22PerMember,
            sy4::Strategy::Sync22Discovery,
        ] {
            for w in [1, 4] {
                println!("SY4 w={w}: {:?}", sy4::run(s, w, 4, 0.3, 200, 10, seed));
            }
        }
    }
    if on("sy5") {
        println!("SY5: {:?}", sy5::run(200, 5_000, seed));
    }
    if on("sy6") {
        let t = Instant::now();
        println!(
            "SY6: {:?} ({} ms)",
            sy6::run(1_000_000, seed),
            t.elapsed().as_millis()
        );
    }
}
