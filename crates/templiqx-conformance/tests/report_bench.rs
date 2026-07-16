//! Thin CI wrapper around report determinism (full 100× stays in the bench binary).

use std::path::Path;

use anyhow::Result;
use templiqx_bench::run_report_determinism;

#[test]
fn cheap_determinism_invariant_holds() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let receipt = run_report_determinism(&root, 5)?;
    assert!(receipt.ok);
    assert_eq!(receipt.distinct_hash_count, 1);
    assert_ne!(receipt.distinct_hash, receipt.sensitivity_hash);
    Ok(())
}
