use coordinator::colony_stats::{all_stat_metrics, enumerate_all_stat_metric_variants};
use std::mem::discriminant;

/// Test that all_stat_metrics() includes all StatMetric variants.
/// 
/// This test ensures that:
/// 1. All variants from StatMetric are included in all_stat_metrics()
/// 2. There are no duplicates
/// 3. The count matches the expected number of variants
/// 
/// If a new StatMetric variant is added to the enum:
/// - The compiler will error in enumerate_all_stat_metric_variants() (exhaustive match)
/// - This test will fail until the variant is added to both functions
#[test]
fn test_all_stat_metrics_completeness() {
    // Get all metrics from the function under test
    let metrics = all_stat_metrics();
    
    // Get all variants using exhaustive match (compiler enforces completeness)
    let all_variants = enumerate_all_stat_metric_variants();
    
    // Check that we have the expected count
    assert_eq!(
        metrics.len(),
        all_variants.len(),
        "all_stat_metrics() should return {} metrics (one for each StatMetric variant)",
        all_variants.len()
    );
    
    // Check for duplicates using discriminant (works even without Eq/Hash)
    for metric in &metrics {
        let disc = discriminant(metric);
        let count = metrics.iter().filter(|m| discriminant(*m) == disc).count();
        assert_eq!(
            count, 1,
            "Duplicate entry found in all_stat_metrics(): {:?} appears {} times",
            metric, count
        );
    }
    
    // Check that each variant from all_variants appears in metrics
    for variant in &all_variants {
        let variant_disc = discriminant(variant);
        let found = metrics.iter().any(|m| discriminant(m) == variant_disc);
        assert!(
            found,
            "StatMetric variant {:?} is missing from all_stat_metrics(). Add it to the function.",
            variant
        );
    }
    
    // Check that metrics doesn't contain any variants not in all_variants
    for metric in &metrics {
        let metric_disc = discriminant(metric);
        let found = all_variants.iter().any(|v| discriminant(v) == metric_disc);
        assert!(
            found,
            "all_stat_metrics() contains unexpected variant {:?} that is not in StatMetric enum",
            metric
        );
    }
}

/// Test that all_stat_metrics() returns metrics in a consistent order.
#[test]
fn test_all_stat_metrics_ordering() {
    let metrics1 = all_stat_metrics();
    let metrics2 = all_stat_metrics();
    
    assert_eq!(
        metrics1.len(),
        metrics2.len(),
        "all_stat_metrics() should return consistent length"
    );
    
    // Compare using discriminant
    for (i, (m1, m2)) in metrics1.iter().zip(metrics2.iter()).enumerate() {
        assert_eq!(
            discriminant(m1),
            discriminant(m2),
            "all_stat_metrics() should return metrics in consistent order. Mismatch at index {}: {:?} != {:?}",
            i, m1, m2
        );
    }
}

