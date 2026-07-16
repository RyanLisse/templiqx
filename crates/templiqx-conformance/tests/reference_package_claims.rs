//! U5 claim test: documented fixture IDs, packages, and corpus artifacts exist.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn packages_root() -> PathBuf {
    repo_root().join("examples/packages")
}

fn corpus_root() -> PathBuf {
    repo_root().join("examples/legacy-corpus/fixtures")
}

fn assert_file(path: &Path) -> Result<()> {
    ensure!(path.is_file(), "missing file: {}", path.display());
    Ok(())
}

fn assert_dir(path: &Path) -> Result<()> {
    ensure!(path.is_dir(), "missing directory: {}", path.display());
    Ok(())
}

struct PackageClaim {
    id: &'static str,
    contracts: &'static [&'static str],
    eval_stems: &'static [&'static str],
    authorized_context: Option<&'static str>,
}

const REFERENCE_PACKAGES: &[PackageClaim] = &[
    PackageClaim {
        id: "basenet-legal",
        contracts: &["legal-matter-extraction", "legal-document-drafting"],
        eval_stems: &[
            "legal-extraction-request",
            "legal-extraction-output",
            "legal-draft-request",
            "legal-draft-output",
        ],
        authorized_context: Some("fixtures/authorized-context.json"),
    },
    PackageClaim {
        id: "finly-advice",
        contracts: &["advice-fact-extraction", "advice-memo-drafting"],
        eval_stems: &[
            "advice-extraction-request",
            "advice-extraction-output",
            "advice-memo-request",
            "advice-memo-output",
        ],
        authorized_context: Some("fixtures/authorized-context.json"),
    },
    PackageClaim {
        id: "simplicate-workflow",
        contracts: &["project-hours-extraction", "invoice-drafting"],
        eval_stems: &[
            "hours-extraction-request",
            "hours-extraction-output",
            "invoice-draft-request",
            "invoice-draft-output",
        ],
        authorized_context: Some("fixtures/authorized-context.json"),
    },
];

const DOCX_RENDER_FIXTURES: &[&str] = &[
    "v5-legal-repeat-rendered",
    "v5-legal-conditional-rendered",
    "v5-nested-table",
    "v5-header-footer",
    "v5-alias-collision-missing",
];

const DOCX_DETECT_ONLY_FIXTURES: &[&str] = &[
    "v5-repeat-marker-detected",
    "v5-conditional-marker-detected",
    "v1-beanshell-detected",
    "v2-marker-detected",
];

const CONTRACT_DOCS: &[&str] = &[
    "docs/contracts/cross-opco-reference-packages-v1alpha1.md",
    "docs/contracts/template-compatibility-report-v1alpha1.md",
];

#[test]
fn reference_package_claims_match_repository() -> Result<()> {
    for doc in CONTRACT_DOCS {
        assert_file(&repo_root().join(doc)).with_context(|| format!("contract doc `{doc}`"))?;
    }

    for claim in REFERENCE_PACKAGES {
        let package_dir = packages_root().join(claim.id);
        assert_dir(&package_dir).with_context(|| format!("package `{}`", claim.id))?;
        assert_file(&package_dir.join("templiqx.yaml"))
            .with_context(|| format!("manifest for `{}`", claim.id))?;

        for contract in claim.contracts {
            assert_file(
                &package_dir
                    .join("contracts")
                    .join(format!("{contract}.yaml")),
            )
            .with_context(|| format!("contract `{contract}` in `{}`", claim.id))?;
        }

        for stem in claim.eval_stems {
            assert_file(&package_dir.join("evals").join(format!("{stem}.json")))
                .with_context(|| format!("eval `{stem}` in `{}`", claim.id))?;
        }

        if let Some(context_path) = claim.authorized_context {
            assert_file(&package_dir.join(context_path)).with_context(|| {
                format!("authorized context for `{}` at `{context_path}`", claim.id)
            })?;
        }
    }

    for fixture in DOCX_RENDER_FIXTURES {
        let dir = corpus_root().join(fixture);
        assert_dir(&dir).with_context(|| format!("render fixture `{fixture}`"))?;
        for file in [
            "source.docx",
            "aliases.json",
            "expected-report.json",
            "render-data.json",
            "expected-render.docx",
        ] {
            assert_file(&dir.join(file))
                .with_context(|| format!("render fixture `{fixture}` file `{file}`"))?;
        }
    }

    for fixture in DOCX_DETECT_ONLY_FIXTURES {
        let dir = corpus_root().join(fixture);
        assert_dir(&dir).with_context(|| format!("detect-only fixture `{fixture}`"))?;
        for file in ["source.docx", "aliases.json", "expected-report.json"] {
            assert_file(&dir.join(file))
                .with_context(|| format!("detect-only fixture `{fixture}` file `{file}`"))?;
        }
    }

    assert_file(&packages_root().join("basenet-legal/templates/v5-legal-template.docx"))?;
    assert_file(&packages_root().join("basenet-legal/templates/draft-email.html"))?;
    assert_file(&packages_root().join("basenet-legal/migrations/v5-aliases.json"))?;

    Ok(())
}

#[test]
fn reference_packages_are_discoverable() -> Result<()> {
    let workspace = tempfile::tempdir()?;
    let service = templiqx_local::compose_with_workspace(packages_root(), workspace.path())
        .context("compose service for reference packages")?;
    let envelope = service.discover_packages();
    ensure!(
        envelope.ok,
        "discover_packages failed: {:?}",
        envelope.diagnostics
    );
    let discovered = envelope
        .result
        .context("discover_packages result")?
        .into_iter()
        .map(|manifest| manifest.package)
        .collect::<Vec<_>>();

    for claim in REFERENCE_PACKAGES {
        ensure!(
            discovered.iter().any(|name| name == claim.id),
            "expected package `{}` in discover_packages",
            claim.id
        );
    }

    Ok(())
}

#[test]
fn documented_contract_docs_reference_fixture_ids() -> Result<()> {
    let reference_doc = fs::read_to_string(
        repo_root().join("docs/contracts/cross-opco-reference-packages-v1alpha1.md"),
    )?;

    for package in ["basenet-legal", "finly-advice", "simplicate-workflow"] {
        ensure!(
            reference_doc.contains(package),
            "cross-opco doc must name package `{package}`"
        );
    }

    for fixture in DOCX_RENDER_FIXTURES
        .iter()
        .chain(DOCX_DETECT_ONLY_FIXTURES.iter())
    {
        ensure!(
            reference_doc.contains(fixture),
            "cross-opco doc must name fixture `{fixture}`"
        );
    }

    let compatibility_doc = fs::read_to_string(
        repo_root().join("docs/contracts/template-compatibility-report-v1alpha1.md"),
    )?;
    ensure!(
        compatibility_doc.contains("templiqx.template-compatibility/v1alpha1"),
        "compatibility report doc must declare api_version"
    );
    ensure!(
        compatibility_doc.contains("approval_handoff"),
        "compatibility report doc must document approval_handoff"
    );

    Ok(())
}
