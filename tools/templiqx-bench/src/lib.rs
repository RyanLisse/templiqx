//! Library surface for report-engine benches (determinism + fan-out).

pub mod report_determinism;
pub mod report_fanout;

pub use report_determinism::{
    DeterminismReceipt, run_report_determinism, run_report_determinism_default,
};
pub use report_fanout::{FanoutReceipt, run_report_fanout};
