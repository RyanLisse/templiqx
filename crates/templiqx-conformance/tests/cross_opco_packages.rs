//! U4: package-driven cross-opco conformance for the three reference packages.

use std::{fs, path::Path};

use anyhow::{Result, ensure};
use serde::Deserialize;
use serde_json::Value;
use templiqx_conformance::file_fingerprint;
use templiqx_contracts::{OperationEnvelope, PackageManifest, TestReport};

const CAPABILITIES: &[&str] = &["structured_output"];

#[derive(Debug, Clone, Copy)]
struct PackageMatrix {
    package: &'static str,
    domain: &'static str,
    channels: &'static [ChannelClaim],
    contract_count: usize,
    eval_count: usize,
}

#[derive(Debug, Clone, Copy)]
enum TemplateClaim {
    Contract(&'static str),
    Artifact(&'static str),
}

#[derive(Debug, Clone, Copy)]
struct ChannelClaim {
    channel: &'static str,
    template: TemplateClaim,
    output: OutputClaim,
    receipt_output: &'static str,
    receipt_contract: &'static str,
    receipt_fixture: &'static str,
}

#[derive(Debug, Clone, Copy)]
enum OutputClaim {
    JsonFields {
        path: &'static str,
        required: &'static [&'static str],
    },
    Docx {
        path: &'static str,
    },
    Pdf {
        path: &'static str,
        manifest: &'static str,
    },
}

const LEGAL_CHANNELS: &[ChannelClaim] = &[
    ChannelClaim {
        channel: "letter",
        template: TemplateClaim::Artifact("templates/v5-legal-template.docx"),
        output: OutputClaim::JsonFields {
            path: "evals/legal-draft-output.json",
            required: &["/title", "/summary", "/merge_data"],
        },
        receipt_output: "evals/legal-draft-output.json",
        receipt_contract: "legal-document-drafting",
        receipt_fixture: "synthetic-legal-draft",
    },
    ChannelClaim {
        channel: "email",
        template: TemplateClaim::Artifact("templates/draft-email.html"),
        output: OutputClaim::JsonFields {
            path: "evals/legal-draft-output.json",
            required: &["/summary", "/merge_data/parties"],
        },
        receipt_output: "evals/legal-draft-output.json",
        receipt_contract: "legal-document-drafting",
        receipt_fixture: "synthetic-legal-draft",
    },
    ChannelClaim {
        channel: "docx",
        template: TemplateClaim::Artifact("templates/v5-legal-template.docx"),
        output: OutputClaim::Docx {
            path: "baselines/v5-legal-approved.docx",
        },
        receipt_output: "evals/legal-draft-output.json",
        receipt_contract: "legal-document-drafting",
        receipt_fixture: "synthetic-legal-draft",
    },
    ChannelClaim {
        channel: "pdf",
        template: TemplateClaim::Artifact("templates/v5-legal-template.docx"),
        output: OutputClaim::Pdf {
            path: "fixtures/recorded-legal.pdf",
            manifest: "fixtures/pdf-renderer-manifest.json",
        },
        receipt_output: "evals/legal-draft-output.json",
        receipt_contract: "legal-document-drafting",
        receipt_fixture: "synthetic-legal-draft",
    },
];

const ADVICE_CHANNELS: &[ChannelClaim] = &[
    ChannelClaim {
        channel: "memo",
        template: TemplateClaim::Contract("advice-memo-drafting"),
        output: OutputClaim::JsonFields {
            path: "evals/advice-memo-output.json",
            required: &["/title", "/summary", "/merge_data"],
        },
        receipt_output: "evals/advice-memo-output.json",
        receipt_contract: "advice-memo-drafting",
        receipt_fixture: "synthetic-advice-memo",
    },
    ChannelClaim {
        channel: "email",
        template: TemplateClaim::Contract("advice-memo-drafting"),
        output: OutputClaim::JsonFields {
            path: "evals/advice-memo-output.json",
            required: &["/email_subject", "/email_body"],
        },
        receipt_output: "evals/advice-memo-output.json",
        receipt_contract: "advice-memo-drafting",
        receipt_fixture: "synthetic-advice-memo",
    },
];

const WORKFLOW_CHANNELS: &[ChannelClaim] = &[
    ChannelClaim {
        channel: "invoice",
        template: TemplateClaim::Contract("invoice-drafting"),
        output: OutputClaim::JsonFields {
            path: "evals/invoice-draft-output.json",
            required: &["/merge_data/lines", "/merge_data/totals"],
        },
        receipt_output: "evals/invoice-draft-output.json",
        receipt_contract: "invoice-drafting",
        receipt_fixture: "synthetic-invoice-draft",
    },
    ChannelClaim {
        channel: "report",
        template: TemplateClaim::Contract("invoice-drafting"),
        output: OutputClaim::JsonFields {
            path: "evals/invoice-draft-output.json",
            required: &["/title", "/summary"],
        },
        receipt_output: "evals/invoice-draft-output.json",
        receipt_contract: "invoice-drafting",
        receipt_fixture: "synthetic-invoice-draft",
    },
    ChannelClaim {
        channel: "sms",
        template: TemplateClaim::Contract("invoice-drafting"),
        output: OutputClaim::JsonFields {
            path: "evals/invoice-draft-output.json",
            required: &["/sms_text"],
        },
        receipt_output: "evals/invoice-draft-output.json",
        receipt_contract: "invoice-drafting",
        receipt_fixture: "synthetic-invoice-draft",
    },
];

const MATRIX: &[PackageMatrix] = &[
    PackageMatrix {
        package: "basenet-legal",
        domain: "legal/basenet",
        channels: LEGAL_CHANNELS,
        contract_count: 2,
        eval_count: 2,
    },
    PackageMatrix {
        package: "finly-advice",
        domain: "regulated-advice",
        channels: ADVICE_CHANNELS,
        contract_count: 2,
        eval_count: 2,
    },
    PackageMatrix {
        package: "simplicate-workflow",
        domain: "project-invoicing",
        channels: WORKFLOW_CHANNELS,
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

        let package_root = packages_root().join(entry.package);
        for channel in entry.channels {
            ensure!(
                channel_coverage(&package_root, manifest, &eval_report, channel),
                "{} missing channel proof for {}",
                entry.package,
                channel.channel
            );
        }
    }
    Ok(())
}

fn channel_coverage(
    package_root: &Path,
    manifest: &PackageManifest,
    eval_report: &TestReport,
    claim: &ChannelClaim,
) -> bool {
    let template_present = match claim.template {
        TemplateClaim::Contract(contract) => {
            manifest.contracts.iter().any(|item| item == contract)
                && package_root
                    .join("contracts")
                    .join(format!("{contract}.yaml"))
                    .is_file()
        }
        TemplateClaim::Artifact(template) => {
            manifest.templates.iter().any(|item| item == template)
                && package_root.join(template).is_file()
        }
    };
    let output_present = channel_output_present(package_root, manifest, claim.output);
    let receipt_output = read_declared_json(package_root, manifest, claim.receipt_output);
    let receipt_fingerprint = receipt_output
        .as_ref()
        .and_then(|output| templiqx_contracts::fingerprint(output).ok());
    let receipt_present = eval_report.cases.iter().any(|case| {
        case.contract_id == claim.receipt_contract
            && case.fixture_id == claim.receipt_fixture
            && case.passed
            && receipt_fingerprint
                .as_ref()
                .is_some_and(|fingerprint| case.artifact_fingerprint.as_ref() == Some(fingerprint))
    });

    template_present && output_present && receipt_present
}

fn channel_output_present(
    package_root: &Path,
    manifest: &PackageManifest,
    output: OutputClaim,
) -> bool {
    match output {
        OutputClaim::JsonFields { path, required } => {
            read_declared_json(package_root, manifest, path).is_some_and(|output| {
                required
                    .iter()
                    .all(|pointer| substantive(output.pointer(pointer)))
            })
        }
        OutputClaim::Docx { path } => fs::read(package_root.join(path))
            .is_ok_and(|bytes| bytes.starts_with(b"PK") && bytes.len() > 4),
        OutputClaim::Pdf { path, manifest } => verify_pdf_output(package_root, path, manifest),
    }
}

fn read_declared_json(
    package_root: &Path,
    manifest: &PackageManifest,
    path: &str,
) -> Option<Value> {
    manifest
        .evals
        .iter()
        .any(|item| item == path)
        .then(|| fs::read(package_root.join(path)).ok())
        .flatten()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

fn substantive(value: Option<&Value>) -> bool {
    match value {
        Some(Value::String(value)) => !value.trim().is_empty(),
        Some(Value::Array(value)) => !value.is_empty(),
        Some(Value::Object(value)) => !value.is_empty(),
        Some(Value::Null) | None => false,
        Some(_) => true,
    }
}

fn verify_pdf_output(package_root: &Path, path: &str, manifest_path: &str) -> bool {
    #[derive(Deserialize)]
    struct PdfManifest {
        source_artifact: String,
        artifact_fingerprint: String,
        artifact_bytes: u64,
        output_hash: String,
    }

    let Some(manifest) = fs::read(package_root.join(manifest_path))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<PdfManifest>(&bytes).ok())
    else {
        return false;
    };
    if manifest.source_artifact != path {
        return false;
    }
    let artifact = package_root.join(path);
    let Ok(bytes) = fs::read(&artifact) else {
        return false;
    };
    let Ok(fingerprint) = file_fingerprint(&artifact) else {
        return false;
    };

    bytes.starts_with(b"%PDF-")
        && u64::try_from(bytes.len()).ok() == Some(manifest.artifact_bytes)
        && fingerprint == manifest.artifact_fingerprint
        && fingerprint == manifest.output_hash
}

#[test]
fn pdf_channel_requires_recorded_artifact_and_manifest() -> Result<()> {
    let packages = packages_root();
    let source = packages.join("basenet-legal");
    let workspace = tempfile::tempdir()?;
    let service = templiqx_local::compose_with_workspace(&packages, workspace.path())?;
    let manifest = result(service.discover_packages())?
        .into_iter()
        .find(|manifest| manifest.package == "basenet-legal")
        .ok_or_else(|| anyhow::anyhow!("missing basenet-legal package"))?;
    let capabilities = CAPABILITIES
        .iter()
        .map(|capability| (*capability).to_string())
        .collect::<Vec<_>>();
    let report = result(service.test_package("basenet-legal", &capabilities))?;
    let root = tempfile::tempdir()?;

    for relative in [
        "templates/v5-legal-template.docx",
        "evals/legal-draft-output.json",
        "fixtures/recorded-legal.pdf",
        "fixtures/pdf-renderer-manifest.json",
    ] {
        let destination = root.path().join(relative);
        fs::create_dir_all(destination.parent().expect("fixture parent"))?;
        fs::copy(source.join(relative), destination)?;
    }

    let claim = LEGAL_CHANNELS
        .iter()
        .find(|claim| claim.channel == "pdf")
        .expect("PDF channel claim");
    ensure!(channel_coverage(root.path(), &manifest, &report, claim));

    fs::remove_file(root.path().join("fixtures/recorded-legal.pdf"))?;
    ensure!(!channel_coverage(root.path(), &manifest, &report, claim));
    fs::copy(
        source.join("fixtures/recorded-legal.pdf"),
        root.path().join("fixtures/recorded-legal.pdf"),
    )?;
    fs::remove_file(root.path().join("fixtures/pdf-renderer-manifest.json"))?;
    ensure!(!channel_coverage(root.path(), &manifest, &report, claim));
    Ok(())
}
