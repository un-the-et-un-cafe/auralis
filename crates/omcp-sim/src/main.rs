//! `cargo run -p omcp-sim --release -- [seed]` prints the SY1–SY6 report.

use std::time::Instant;

use omcp_sim::{sy1, sy23, sy4, sy5, sy6};

fn main() {
    let seed: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0x5EED);
    println!("seed = {seed:#x}");
    for n in [1_000, 10_000] {
        let t = Instant::now();
        let r = sy1::run(&sy1::Sy1Params::new(n, seed));
        println!("SY1 n={n}: {r:?} ({} ms)", t.elapsed().as_millis());
    }
    println!("SY2: {:?}", sy23::sy2(100_000, seed));
    println!("SY3: {:?}", sy23::sy3(55, 500, 10, seed));
    for s in [
        sy4::Strategy::Flood,
        sy4::Strategy::Interest,
        sy4::Strategy::SprayOnly,
    ] {
        for w in [1, 4] {
            println!("SY4 w={w}: {:?}", sy4::run(s, w, 4, 0.3, 2_000, seed));
        }
    }
    println!("SY5: {:?}", sy5::run(200, 5_000, seed));
    let t = Instant::now();
    println!(
        "SY6: {:?} ({} ms)",
        sy6::run(500_000, seed),
        t.elapsed().as_millis()
    );
}
