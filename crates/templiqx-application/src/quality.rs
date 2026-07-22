//! Canonical application binding for proposal-only quality assessment.
//!
//! This module deliberately owns only package/contract binding and candidate
//! preparation. Metric validation, aggregation, eligibility, and Pareto
//! semantics remain pure functions in `templiqx-core`.

use super::*;
use templiqx_contracts::{
    ComputedQualityIdentities, QUALITY_MAX_CANDIDATE_SOURCE_BYTES, QUALITY_MAX_CANDIDATES,
    QUALITY_MAX_FIXTURES, QUALITY_MAX_REQUEST_BYTES, QualityCandidateFingerprintPayload,
    QualityProposalReport, QualityProposalReportPayload, QualityProposalRequest,
    QualityRequestFingerprintPayload, TQX_QUALITY_BINDING_MISMATCH, TQX_QUALITY_EVIDENCE_INVALID,
    TQX_QUALITY_LIMIT_EXCEEDED,
};

/// Return every changed top-level `Contract` field as a sorted JSON-pointer.
///
/// This is intentionally separate from the legacy `diff_contract` operation:
/// quality proposals need complete change disclosure without changing the
/// existing operation's response semantics.
pub(crate) fn proposal_change_paths(base: &Contract, candidate: &Contract) -> Vec<String> {
    let mut paths = Vec::new();
    macro_rules! changed {
        ($field:ident) => {
            if base.$field != candidate.$field {
                paths.push(concat!("/", stringify!($field)).to_owned());
            }
        };
    }

    changed!(api_version);
    changed!(id);
    changed!(version);
    changed!(description);
    changed!(inputs);
    changed!(context);
    changed!(capabilities);
    changed!(messages);
    changed!(output_schema);
    changed!(runtime_policy);
    changed!(extensions);
    changed!(components);
    changed!(provenance);
    changed!(evals);
    paths.sort();
    paths
}

fn quality_diagnostic(code: &str, pointer: impl Into<String>) -> Diagnostic {
    let message = match code {
        templiqx_contracts::TQX_QUALITY_BASE_STALE => "the expected base contract is not current",
        templiqx_contracts::TQX_QUALITY_BINDING_MISMATCH => {
            "quality evidence does not match the current artifact identity"
        }
        templiqx_contracts::TQX_QUALITY_CANDIDATE_INVALID => "candidate contract is invalid",
        templiqx_contracts::TQX_QUALITY_LIMIT_EXCEEDED => {
            "quality proposal exceeds a protocol limit"
        }
        _ => "quality proposal is invalid",
    };
    Diagnostic::error(code, message, pointer)
}

fn quality_snapshot_failure<T>(
    operation: &str,
    error: PortError,
    pointer: impl Into<String>,
) -> OperationEnvelope<T> {
    let mut diagnostic = port_diagnostic(error);
    diagnostic.message = "the package snapshot could not be read".into();
    diagnostic.json_pointer = Some(pointer.into());
    diagnostic.file = None;
    diagnostic.help = None;
    OperationEnvelope::new(operation, None, vec![diagnostic])
}

/// Collapse parser/validator details to bounded stable code/path pairs. In
/// particular, never retain parser excerpts, candidate values, source spans,
/// files, or help text that could echo submitted source.
fn redact_candidate_diagnostics(_found: &[Diagnostic], candidate_index: usize) -> Vec<Diagnostic> {
    vec![quality_diagnostic(
        templiqx_contracts::TQX_QUALITY_CANDIDATE_INVALID,
        format!("/candidates/{candidate_index}/candidate_source"),
    )]
}

fn normalized_identity_manifest(manifest: &PackageManifest) -> PackageManifest {
    let mut normalized = manifest.clone();
    normalized.signatures.clear();
    normalized.contracts.sort();
    normalized.components.sort();
    normalized.evals.sort();
    normalized.migrations.sort();
    normalized.templates.sort();
    normalized.translations.sort();
    normalized.definitions.sort();
    normalized
}

fn manifest_matches_identity(manifest: &PackageManifest, identity: &PackageIdentity) -> bool {
    normalized_identity_manifest(manifest) == normalized_identity_manifest(&identity.manifest)
}

impl<S, W, R, L, D, I> TempliqxService<S, W, R, L, D, I>
where
    S: PackageStore,
    W: ArtifactWorkspace,
    R: RuntimeAdapter,
    L: LegacyImportAdapter,
    D: DocumentRenderer,
    I: DocumentInspector,
{
    /// Validate and assess full contract proposals without mutating package
    /// storage. Model calls, retries, optimizer execution, approval, and the
    /// later CAS-protected write remain host-owned.
    pub fn assess_quality_proposals(
        &self,
        request: &QualityProposalRequest,
    ) -> OperationEnvelope<QualityProposalReport> {
        const OPERATION: &str = "assess_quality_proposals";

        let request_size = match serde_json::to_vec(request) {
            Ok(encoded) => encoded.len(),
            Err(error) => return serialization_failure(OPERATION, error),
        };
        if request_size > QUALITY_MAX_REQUEST_BYTES {
            return OperationEnvelope::new(
                OPERATION,
                None,
                vec![quality_diagnostic(TQX_QUALITY_LIMIT_EXCEEDED, "/")],
            );
        }
        if request.candidates.is_empty() || request.candidates.len() > QUALITY_MAX_CANDIDATES {
            return OperationEnvelope::new(
                OPERATION,
                None,
                vec![quality_diagnostic(
                    TQX_QUALITY_LIMIT_EXCEEDED,
                    "/candidates",
                )],
            );
        }
        let validated_policy = match templiqx_core::validate_quality_policy(&request.policy) {
            Ok(policy) => policy,
            Err(diagnostics) => return OperationEnvelope::new(OPERATION, None, diagnostics),
        };

        // The manifest is an operation-scoped snapshot. In particular, tool
        // contracts must not be reloaded independently for each candidate.
        let manifest = match self.store.manifest(&request.package) {
            Ok(manifest) => manifest,
            Err(error) => {
                return quality_snapshot_failure(OPERATION, error, "/package_identity/manifest");
            }
        };
        let package_identity = match self.canonical_package_identity(&request.package) {
            Ok(identity) => identity,
            Err(error) => return quality_snapshot_failure(OPERATION, error, "/package_identity"),
        };
        if !manifest_matches_identity(&manifest, &package_identity) {
            return OperationEnvelope::new(
                OPERATION,
                None,
                vec![quality_diagnostic(
                    TQX_QUALITY_BINDING_MISMATCH,
                    "/package_identity/manifest",
                )],
            );
        }
        let base_source = match self
            .store
            .contract_source(&request.package, &request.contract_id)
        {
            Ok(source) => source,
            Err(error) => {
                return quality_snapshot_failure(OPERATION, error, "/package_identity/artifacts");
            }
        };
        let base_artifact = format!("contracts/{}.yaml", request.contract_id);
        if package_identity.artifacts.get(&base_artifact)
            != Some(&fingerprint_bytes(base_source.as_bytes()))
        {
            return OperationEnvelope::new(
                OPERATION,
                None,
                vec![quality_diagnostic(
                    TQX_QUALITY_BINDING_MISMATCH,
                    "/package_identity/artifacts",
                )],
            );
        }
        let base = match self.prepare_quality_candidate(
            &request.package,
            &base_source,
            &manifest.tool_contracts,
        ) {
            Ok(contract) => contract,
            Err(diagnostics) => return OperationEnvelope::new(OPERATION, None, diagnostics),
        };
        if base.evals.is_empty() || base.evals.len() > QUALITY_MAX_FIXTURES {
            return OperationEnvelope::new(
                OPERATION,
                None,
                vec![quality_diagnostic(
                    TQX_QUALITY_LIMIT_EXCEEDED,
                    "/base_contract/evals",
                )],
            );
        }

        let package_fingerprint =
            match templiqx_contracts::quality_package_fingerprint(&package_identity) {
                Ok(value) => value,
                Err(error) => return serialization_failure(OPERATION, error),
            };
        let base_contract_fingerprint = match fingerprint(&base) {
            Ok(value) => value,
            Err(error) => return serialization_failure(OPERATION, error),
        };
        let fixture_set_fingerprint =
            match templiqx_contracts::quality_fixture_set_fingerprint(&base.evals) {
                Ok(value) => value,
                Err(_) => {
                    return OperationEnvelope::new(
                        OPERATION,
                        None,
                        vec![quality_diagnostic(
                            TQX_QUALITY_EVIDENCE_INVALID,
                            "/base_contract/evals",
                        )],
                    );
                }
            };
        let policy_fingerprint =
            match templiqx_contracts::quality_policy_fingerprint(&request.policy) {
                Ok(value) => value,
                Err(error) => return serialization_failure(OPERATION, error),
            };

        if request.expected_base_contract_fingerprint != base_contract_fingerprint {
            return OperationEnvelope::new(
                OPERATION,
                None,
                vec![quality_diagnostic(
                    templiqx_contracts::TQX_QUALITY_BASE_STALE,
                    "/expected_base_contract_fingerprint",
                )],
            );
        }
        let mut binding_diagnostics = Vec::new();
        if request.expected_package_fingerprint != package_fingerprint {
            binding_diagnostics.push(quality_diagnostic(
                TQX_QUALITY_BINDING_MISMATCH,
                "/expected_package_fingerprint",
            ));
        }
        if request.expected_fixture_set_fingerprint != fixture_set_fingerprint {
            binding_diagnostics.push(quality_diagnostic(
                TQX_QUALITY_BINDING_MISMATCH,
                "/expected_fixture_set_fingerprint",
            ));
        }
        if !binding_diagnostics.is_empty() {
            return OperationEnvelope::new(OPERATION, None, binding_diagnostics);
        }

        let mut prepared = Vec::with_capacity(request.candidates.len());
        let mut fingerprint_candidates = Vec::with_capacity(request.candidates.len());
        let expected_measurement_profiles: std::collections::BTreeMap<_, _> = request
            .policy
            .objectives
            .iter()
            .map(|objective| {
                (
                    objective.metric_id.clone(),
                    objective.claimed_measurement_profile_fingerprint.clone(),
                )
            })
            .collect();
        let mut submissions: Vec<_> = request.candidates.iter().collect();
        submissions.sort_by(|left, right| {
            left.evidence
                .claimed_candidate_contract_fingerprint
                .cmp(&right.evidence.claimed_candidate_contract_fingerprint)
        });
        if submissions.windows(2).any(|pair| {
            pair[0].evidence.claimed_candidate_contract_fingerprint
                == pair[1].evidence.claimed_candidate_contract_fingerprint
        }) {
            return OperationEnvelope::new(
                OPERATION,
                None,
                vec![quality_diagnostic(
                    TQX_QUALITY_EVIDENCE_INVALID,
                    "/candidates",
                )],
            );
        }
        for (index, submission) in submissions.into_iter().enumerate() {
            let mut prevalidation_diagnostics = Vec::new();
            if submission.candidate_source.len() > QUALITY_MAX_CANDIDATE_SOURCE_BYTES {
                return OperationEnvelope::new(
                    OPERATION,
                    None,
                    vec![quality_diagnostic(
                        TQX_QUALITY_LIMIT_EXCEEDED,
                        format!("/candidates/{index}/candidate_source"),
                    )],
                );
            }
            if !submission.synthetic_or_sanitized_data_attestation {
                prevalidation_diagnostics.push(quality_diagnostic(
                    templiqx_contracts::TQX_QUALITY_CANDIDATE_INVALID,
                    format!("/candidates/{index}/synthetic_or_sanitized_data_attestation"),
                ));
            }

            let parsed = self.prepare_quality_candidate(
                &request.package,
                &submission.candidate_source,
                &manifest.tool_contracts,
            );
            let (candidate_fingerprint, proposal_change_paths) = match parsed {
                Ok(candidate) => {
                    let paths = proposal_change_paths(&base, &candidate);
                    match fingerprint(&candidate) {
                        Ok(candidate_fingerprint) => {
                            match templiqx_contracts::quality_fixture_set_fingerprint(
                                &candidate.evals,
                            ) {
                                Ok(candidate_fixture_fingerprint)
                                    if candidate_fixture_fingerprint == fixture_set_fingerprint => {
                                }
                                _ => prevalidation_diagnostics.push(quality_diagnostic(
                                    TQX_QUALITY_BINDING_MISMATCH,
                                    format!("/candidates/{index}/candidate_source/evals"),
                                )),
                            }
                            (Some(candidate_fingerprint), paths)
                        }
                        Err(error) => return serialization_failure(OPERATION, error),
                    }
                }
                Err(found) => {
                    prevalidation_diagnostics.extend(redact_candidate_diagnostics(&found, index));
                    (None, Vec::new())
                }
            };
            for (matches, field) in [
                (
                    submission.evidence.claimed_package_fingerprint == package_fingerprint,
                    "claimed_package_fingerprint",
                ),
                (
                    submission.evidence.claimed_base_contract_fingerprint
                        == base_contract_fingerprint,
                    "claimed_base_contract_fingerprint",
                ),
                (
                    submission.evidence.claimed_fixture_set_fingerprint == fixture_set_fingerprint,
                    "claimed_fixture_set_fingerprint",
                ),
                (
                    submission.evidence.claimed_quality_policy_fingerprint == policy_fingerprint,
                    "claimed_quality_policy_fingerprint",
                ),
            ] {
                if !matches {
                    prevalidation_diagnostics.push(quality_diagnostic(
                        TQX_QUALITY_BINDING_MISMATCH,
                        format!("/candidates/{index}/evidence/{field}"),
                    ));
                }
            }
            if candidate_fingerprint.as_ref().is_some_and(|computed| {
                submission.evidence.claimed_candidate_contract_fingerprint != *computed
            }) {
                prevalidation_diagnostics.push(quality_diagnostic(
                    TQX_QUALITY_BINDING_MISMATCH,
                    format!("/candidates/{index}/evidence/claimed_candidate_contract_fingerprint"),
                ));
            }
            if submission.evidence.claimed_measurement_profile_fingerprints
                != expected_measurement_profiles
            {
                prevalidation_diagnostics.push(quality_diagnostic(
                    TQX_QUALITY_BINDING_MISMATCH,
                    format!(
                        "/candidates/{index}/evidence/claimed_measurement_profile_fingerprints"
                    ),
                ));
            }
            prevalidation_diagnostics.sort_by(|left, right| {
                (&left.code, &left.json_pointer).cmp(&(&right.code, &right.json_pointer))
            });
            prevalidation_diagnostics.dedup_by(|left, right| {
                left.code == right.code && left.json_pointer == right.json_pointer
            });

            fingerprint_candidates.push(QualityCandidateFingerprintPayload {
                candidate_contract_fingerprint: candidate_fingerprint.clone(),
                synthetic_or_sanitized_data_attestation: submission
                    .synthetic_or_sanitized_data_attestation,
                evidence: submission.evidence.clone(),
            });
            prepared.push(templiqx_core::PreparedQualityCandidate {
                candidate_fingerprint,
                evidence: submission.evidence.clone(),
                proposal_change_paths,
                prevalidation_diagnostics,
            });
        }

        let request_fingerprint = match templiqx_contracts::quality_request_fingerprint(
            &QualityRequestFingerprintPayload {
                package: request.package.clone(),
                contract_id: request.contract_id.clone(),
                expected_package_fingerprint: request.expected_package_fingerprint.clone(),
                expected_base_contract_fingerprint: request
                    .expected_base_contract_fingerprint
                    .clone(),
                expected_fixture_set_fingerprint: request.expected_fixture_set_fingerprint.clone(),
                policy: request.policy.clone(),
                candidates: fingerprint_candidates,
            },
        ) {
            Ok(value) => value,
            Err(error) => return serialization_failure(OPERATION, error),
        };

        let fixture_ids: Vec<_> = base
            .evals
            .iter()
            .map(|fixture| fixture.id.clone())
            .collect();
        let mut assessment = match templiqx_core::assess_quality_candidates(
            &validated_policy,
            &fixture_ids,
            prepared,
        ) {
            Ok(result) => result,
            Err(diagnostics) => return OperationEnvelope::new(OPERATION, None, diagnostics),
        };
        for (index, candidate) in assessment.candidate_assessments.iter_mut().enumerate() {
            if candidate.candidate_fingerprint.is_none()
                && candidate.diagnostics.iter().any(|diagnostic| {
                    diagnostic.code == templiqx_contracts::TQX_QUALITY_CANDIDATE_INVALID
                })
            {
                let stable_pointer = candidate
                    .diagnostics
                    .iter()
                    .filter(|diagnostic| {
                        diagnostic.code == templiqx_contracts::TQX_QUALITY_CANDIDATE_INVALID
                    })
                    .filter_map(|diagnostic| diagnostic.json_pointer.as_deref())
                    .find(|pointer| pointer.starts_with("/candidates/"))
                    .map_or_else(
                        || format!("/candidates/{index}/candidate_source"),
                        ToOwned::to_owned,
                    );
                candidate.diagnostics.retain(|diagnostic| {
                    diagnostic.code != templiqx_contracts::TQX_QUALITY_CANDIDATE_INVALID
                });
                candidate.diagnostics.push(quality_diagnostic(
                    templiqx_contracts::TQX_QUALITY_CANDIDATE_INVALID,
                    stable_pointer,
                ));
                candidate.diagnostics = templiqx_core::bounded_quality_diagnostics(std::mem::take(
                    &mut candidate.diagnostics,
                ));
            }
        }
        let computed_identities = ComputedQualityIdentities {
            package_fingerprint: package_fingerprint.clone(),
            base_contract_fingerprint: base_contract_fingerprint.clone(),
            fixture_set_fingerprint: fixture_set_fingerprint.clone(),
            quality_policy_fingerprint: policy_fingerprint.clone(),
            request_fingerprint: request_fingerprint.clone(),
        };
        let payload = QualityProposalReportPayload {
            computed_identities: computed_identities.clone(),
            candidate_assessments: assessment.candidate_assessments,
            pareto_fronts: assessment.pareto_fronts,
        };
        let report_fingerprint = match templiqx_contracts::quality_report_fingerprint(&payload) {
            Ok(value) => value,
            Err(error) => return serialization_failure(OPERATION, error),
        };
        let report = QualityProposalReport {
            computed_identities,
            candidate_assessments: payload.candidate_assessments,
            pareto_fronts: payload.pareto_fronts,
            report_fingerprint: report_fingerprint.clone(),
        };
        let final_package_identity = match self.canonical_package_identity(&request.package) {
            Ok(identity) => identity,
            Err(error) => return quality_snapshot_failure(OPERATION, error, "/package_identity"),
        };
        if final_package_identity != package_identity {
            return OperationEnvelope::new(
                OPERATION,
                None,
                vec![quality_diagnostic(
                    TQX_QUALITY_BINDING_MISMATCH,
                    "/package_identity",
                )],
            );
        }
        OperationEnvelope::new(OPERATION, Some(report), vec![])
            .fingerprint("package_identity", package_fingerprint)
            .fingerprint("base_contract", base_contract_fingerprint)
            .fingerprint("fixture_set", fixture_set_fingerprint)
            .fingerprint("quality_policy", policy_fingerprint)
            .fingerprint("request", request_fingerprint)
            .fingerprint("report", report_fingerprint)
    }

    fn prepare_quality_candidate(
        &self,
        package: &str,
        source: &str,
        tool_contracts: &std::collections::BTreeMap<String, templiqx_contracts::ToolContractRef>,
    ) -> Result<Contract, Vec<Diagnostic>> {
        let mut candidate = templiqx_core::parse_contract(source, None)?;
        if !tool_contracts.is_empty() {
            let found = templiqx_core::resolve_tool_contract_refs(&mut candidate, tool_contracts);
            if found
                .iter()
                .any(|diagnostic| diagnostic.severity == Severity::Error)
            {
                return Err(found);
            }
        }
        for message in &mut candidate.messages {
            let content = std::mem::take(&mut message.content);
            message.content = self.expand_includes(package, content, &mut Vec::new())?;
        }
        for definition in candidate.components.values_mut() {
            match definition {
                templiqx_contracts::ComponentDefinition::Typed(component) => {
                    let content = std::mem::take(&mut component.content);
                    component.content = self.expand_includes(package, content, &mut Vec::new())?;
                }
                templiqx_contracts::ComponentDefinition::Legacy(nodes) => {
                    let content = std::mem::take(nodes);
                    *nodes = self.expand_includes(package, content, &mut Vec::new())?;
                }
            }
        }
        let diagnostics = templiqx_core::validate_contract(&candidate);
        if diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error)
        {
            return Err(diagnostics);
        }
        Ok(candidate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo_contract() -> Contract {
        let source = include_str!("../../../examples/packages/demo/contracts/greeting.yaml");
        templiqx_core::parse_contract(source, None).expect("demo contract")
    }

    #[test]
    fn proposal_change_paths_cover_all_contract_fields_and_sort() {
        let base = demo_contract();
        let mut changed = base.clone();
        changed.api_version.push_str("-changed");
        changed.id.push_str("-changed");
        changed.version = "9.9.9".into();
        changed.description.push_str(" changed");
        changed.inputs.clear();
        changed.context.clear();
        changed.capabilities.clear();
        changed.messages.clear();
        changed.output_schema = serde_json::json!({"changed": true});
        changed.runtime_policy.clear();
        changed.extensions.insert(
            "changed".into(),
            templiqx_contracts::ExtensionSpec {
                capability: "changed".into(),
                schema: serde_json::json!({}),
                value: serde_json::json!({}),
            },
        );
        changed.components.clear();
        changed.provenance.clear();
        changed.evals.clear();

        assert_eq!(
            proposal_change_paths(&base, &changed),
            vec![
                "/api_version",
                "/capabilities",
                "/components",
                "/context",
                "/description",
                "/evals",
                "/extensions",
                "/id",
                "/inputs",
                "/messages",
                "/output_schema",
                "/provenance",
                "/runtime_policy",
                "/version",
            ]
        );
    }

    #[test]
    fn candidate_diagnostics_are_stable_redacted_and_deduplicated() {
        let raw = vec![
            Diagnostic::error("TQX_PARSE_YAML", "secret: CANARY", "/messages"),
            Diagnostic::error("TQX_OTHER", "other CANARY", "/messages"),
        ];
        let diagnostics = redact_candidate_diagnostics(&raw, 2);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].code,
            templiqx_contracts::TQX_QUALITY_CANDIDATE_INVALID
        );
        assert_eq!(
            diagnostics[0].json_pointer.as_deref(),
            Some("/candidates/2/candidate_source")
        );
        let encoded = serde_json::to_string(&diagnostics).expect("diagnostics JSON");
        assert!(!encoded.contains("CANARY"));
        assert!(!encoded.contains("secret"));
    }

    #[test]
    fn candidate_controlled_pointers_collapse_to_one_fixed_path() {
        let raw = vec![
            Diagnostic::error("TQX_PARSE_YAML", "CANARY", "/inputs/PII-CANARY"),
            Diagnostic::error("TQX_PARSE_YAML", "CANARY", "/components/PII-CANARY"),
            Diagnostic::error("TQX_PARSE_YAML", "CANARY", "/extensions/PII-CANARY"),
        ];
        let diagnostics = redact_candidate_diagnostics(&raw, 0);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].json_pointer.as_deref(),
            Some("/candidates/0/candidate_source")
        );
        assert!(
            !serde_json::to_string(&diagnostics)
                .expect("diagnostics JSON")
                .contains("CANARY")
        );
    }
}
