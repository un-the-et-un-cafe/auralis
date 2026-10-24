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
fn sy4_six_hops_30pct_churn_98pct_in_10_minutes() {
    for seed in SEEDS {
        for width in [1, 4] {
            let flood = sy4::run(sy4::Strategy::Flood, width, 4, 0.3, 500, seed);
            let interest = sy4::run(sy4::Strategy::Interest, width, 4, 0.3, 500, seed);
            assert!(interest.delivery_rate >= 0.98, "{interest:?}");
            assert!(
                interest.delivery_rate >= flood.delivery_rate - 0.01,
                "{interest:?} {flood:?}"
            );
            assert!(interest.mean_transmissions <= flood.mean_transmissions);
        }
    }
}

#[test]
fn sy4_pure_spray_and_wait_does_not_meet_target_on_static_path() {
    // Documented finding: without carry tags the §22.1 fallback alone does
    // not reach a 6-hop destination on a static path.
    let r = sy4::run(sy4::Strategy::SprayOnly, 4, 4, 0.3, 300, SEEDS[0]);
    assert!(r.delivery_rate < 0.98);
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
fn sy6_one_million_tags_unlinkable() {
    let r = sy6::run(500_000, SEEDS[0]);
    assert_eq!(r.tags, 1_000_000);
    assert!(r.max_bit_z < 5.0, "{r:?}");
    assert!(r.max_xor_bit_z < 5.0, "{r:?}");
    assert!(r.hamming_z.abs() < 5.0, "{r:?}");
    assert!(r.best_distinguisher_advantage < 0.005, "{r:?}");
    assert_eq!(r.collisions, 0);
}
