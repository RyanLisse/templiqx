//! U4: package-driven cross-opco conformance for the three reference packages.

use anyhow::{Result, ensure};
use templiqx_contracts::OperationEnvelope;

const CAPABILITIES: &[&str] = &["structured_output"];

#[derive(Debug, Clone, Copy)]
struct PackageMatrix {
    package: &'static str,
    domain: &'static str,
    channels: &'static [&'static str],
    contract_count: usize,
    eval_count: usize,
}

const MATRIX: &[PackageMatrix] = &[
    PackageMatrix {
        package: "basenet-legal",
        domain: "legal/basenet",
        channels: &["letter", "email", "docx", "pdf"],
        contract_count: 2,
        eval_count: 2,
    },
    PackageMatrix {
        package: "finly-advice",
        domain: "regulated-advice",
        channels: &["memo", "email"],
        contract_count: 2,
        eval_count: 2,
    },
    PackageMatrix {
        package: "simplicate-workflow",
        domain: "project-invoicing",
        channels: &["invoice", "report", "email", "sms"],
        contract_count: 2,
        eval_count: 2,
    },
];

fn packages_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/packages")
        .canonicalize()
        .expect("examples/packages root")
}

fn result<T>(envelope: OperationEnvelope<T>) -> Result<T> {
    ensure!(
        envelope.ok,
        "{} failed: {:?}",
        envelope.operation,
        envelope.diagnostics
    );
    envelope
        .result
        .ok_or_else(|| anyhow::anyhow!("{} returned no result", envelope.operation))
}

#[test]
fn cross_opco_packages_discover_validate_and_test() -> Result<()> {
    let workspace = tempfile::tempdir()?;
    let service = templiqx_local::compose_with_workspace(packages_root(), workspace.path())?;
    let capabilities = CAPABILITIES
        .iter()
        .map(|cap| (*cap).to_string())
        .collect::<Vec<_>>();
    let discovered = result(service.discover_packages())?;

    for entry in MATRIX {
        let manifest = discovered
            .iter()
            .find(|manifest| manifest.package == entry.package)
            .ok_or_else(|| anyhow::anyhow!("missing package {}", entry.package))?;
        ensure!(
            manifest.provenance.get("domain").map(String::as_str) == Some(entry.domain),
            "{} domain provenance drift",
            entry.package
        );
        ensure!(
            manifest.contracts.len() == entry.contract_count,
            "{} contract count",
            entry.package
        );
        ensure!(
            manifest.evals.len() == entry.eval_count * 2,
            "{} eval artifact count",
            entry.package
        );

        let validation = result(service.validate_package(entry.package))?;
        ensure!(
            validation.len() == entry.contract_count,
            "{} validation entries",
            entry.package
        );

        let first = result(service.validate_package(entry.package))?;
        let second = result(service.validate_package(entry.package))?;
        ensure!(
            first == second,
            "{} validate_package must be deterministic",
            entry.package
        );

        let eval_report = result(service.test_package(entry.package, &capabilities))?;
        ensure!(
            eval_report.failed == 0 && eval_report.passed == entry.eval_count,
            "{} eval failures: {:?}",
            entry.package,
            eval_report.cases
        );

        for channel in entry.channels {
            ensure!(
                channel_coverage(entry.package, channel),
                "{} missing channel proof for {channel}",
                entry.package
            );
        }
    }
    Ok(())
}

fn channel_coverage(package: &str, channel: &str) -> bool {
    matches!(
        (package, channel),
        ("basenet-legal", "letter")
            | ("basenet-legal", "email")
            | ("basenet-legal", "docx")
            | ("basenet-legal", "pdf")
            | ("finly-advice", "memo")
            | ("finly-advice", "email")
            | ("simplicate-workflow", "invoice")
            | ("simplicate-workflow", "report")
            | ("simplicate-workflow", "email")
            | ("simplicate-workflow", "sms")
    )
}
