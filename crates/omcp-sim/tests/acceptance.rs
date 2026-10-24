//! PROTOCOL §22.6 acceptance tests SY1–SY6 (deterministic, fixed seeds).

use omcp_sim::{sy1, sy23, sy4, sy5, sy6};

const SEEDS: [u64; 2] = [0x5EED, 0xC0FFEE];

#[test]
fn sy1_10k_nodes_median_under_5mb_and_sublinear_growth() {
    for seed in SEEDS {
        let small = sy1::run(&sy1::Sy1Params::new(1_000, seed));
        let big = sy1::run(&sy1::Sy1Params::new(10_000, seed));
        assert!(big.median_bytes <= 5_000_000, "{big:?}");
        let growth = big.median_bytes as f64 / small.median_bytes as f64;
        assert!(growth < 1.5, "growth {growth}: {small:?} {big:?}");
        // Flooding baseline grows ~linearly; interest routing stays flat.
        assert!(big.flood_median_bytes > 5 * big.median_bytes);
        assert!(big.messages > 0 && big.digests_sent > 0);
    }
}

#[test]
fn sy1_idle_overhead_under_1mb_at_8_neighbours() {
    for seed in SEEDS {
        let r = sy1::run(&sy1::Sy1Params::new(10_000, seed));
        assert_eq!(r.radio_neighbours, 8);
        assert!(r.idle_overhead_p95 <= 1_000_000, "{r:?}");
    }
}

#[test]
fn sy1_finding_metadata_budget_gaps() {
    // Documented findings (see docs/SYNC-v0.2.md):
    // - at 24 radio neighbours idle overhead exceeds 1 MB/day (5-min full
    //   digest refresh received from every neighbour dominates);
    // - for active nodes metadata is > 2 % of their own payload, both raw
    //   and above the idle baseline.
    let mut p = sy1::Sy1Params::new(10_000, SEEDS[0]);
    p.radio_radius = 2;
    let r = sy1::run(&p);
    assert!(r.idle_overhead_median > 1_000_000, "{r:?}");
    assert!(r.active_meta_ratio_raw_median > 0.02, "{r:?}");
    assert!(r.active_meta_ratio_incremental_median > 0.02, "{r:?}");
}

#[test]
fn sy1_is_deterministic() {
    let a = sy1::run(&sy1::Sy1Params::new(500, 7));
    let b = sy1::run(&sy1::Sy1Params::new(500, 7));
    assert_eq!(format!("{a:?}"), format!("{b:?}"));
}

#[test]
fn sy2_in_sync_reconnect_under_256_bytes() {
    for seed in SEEDS {
        for items in [0, 1, 1_000, 100_000] {
            let r = sy23::sy2(items, seed);
            assert!(r.total_bytes <= 256, "{r:?}");
            assert_eq!(r.round_trips, 1);
        }
    }
}

#[test]
fn sy3_week_offline_catch_up_within_1_2x() {
    for seed in SEEDS {
        let r = sy23::sy3(55, 500, 10, seed);
        assert!(r.ratio <= 1.2, "{r:?}");
        assert_eq!(r.foreign_bytes, 0);
        assert!(r.missed_envelopes >= 55 * 70);
    }
}

#[test]
fn sy4_six_hops_30pct_churn_98pct_with_proposed_discovery() {
    for seed in SEEDS {
        for width in [1, 4] {
            let flood = sy4::run(sy4::Strategy::Flood, width, 4, 0.3, 60, 10, seed);
            let d = sy4::run(sy4::Strategy::Sync22Discovery, width, 4, 0.3, 60, 10, seed);
            assert!(d.delivery_rate >= 0.98, "{d:?}");
            assert!(d.cold_delivery_rate >= 0.98, "{d:?}");
            assert!(
                d.delivery_rate >= flood.delivery_rate - 0.01,
                "{d:?} {flood:?}"
            );
            assert!(d.mean_transmissions_per_msg < flood.mean_transmissions_per_msg);
        }
    }
}

#[test]
fn sy4_finding_section_22_7_1_as_written_misses_target() {
    // Documented finding: with the §22.7(1) rules as written (activity-gated
    // carry tags on the shared chat tag) delivery on a static 6-hop path is
    // far below 98 %; cold start is 0 %.
    for width in [1, 4] {
        let r = sy4::run(sy4::Strategy::Sync22, width, 4, 0.3, 60, 10, SEEDS[0]);
        assert!(r.delivery_rate < 0.98, "{r:?}");
        assert_eq!(r.cold_delivery_rate, 0.0, "{r:?}");
    }
}

#[test]
fn sy5_budgets_never_exceeded_and_eviction_order() {
    for seed in SEEDS {
        let r = sy5::run(20, 3_000, seed);
        assert!(r.max_fill_ratio <= 1.0, "{r:?}");
        assert_eq!(r.order_violations, 0, "{r:?}");
        assert_eq!(r.forward_cap_violations, 0, "{r:?}");
        assert!(r.evictions > 0);
    }
}

#[test]
fn sy6_one_million_pairs_unlinkable() {
    // §22.7(8): ≥ 10^6 tag pairs, advantage < 0.005, |z| < 5.
    let r = sy6::run(1_000_000, SEEDS[0]);
    assert_eq!(r.tags, 2_000_000);
    assert!(r.max_bit_z < 5.0, "{r:?}");
    assert!(r.max_xor_bit_z < 5.0, "{r:?}");
    assert!(r.hamming_z.abs() < 5.0, "{r:?}");
    assert!(r.best_distinguisher_advantage < 0.005, "{r:?}");
    assert_eq!(r.collisions, 0);
}
