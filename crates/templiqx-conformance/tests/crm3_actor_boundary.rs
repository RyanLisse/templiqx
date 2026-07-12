use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use templiqx_application::TempliqxService;
use templiqx_contracts::{
    Diagnostic, ExecutionReceipt, OperationEnvelope, RenderRequest, Severity,
};
use templiqx_local::{
    FilesystemArtifactWorkspace, FilesystemPackageStore, UnsupportedDocumentRenderer,
    UnsupportedLegacyAdapter,
};
use templiqx_mock::{ScriptedRuntime, ScriptedScenario};
use templiqx_ports::RuntimeFailureCode;

const PACKAGE: &str = "crm3";
const CONTRACT: &str = "bli-61-date-term-extraction";
const TENANT: &str = "tenant-crm3";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Actor {
    Human,
    Agent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum HostRejection {
    DirectAgentCommitDenied,
    ProposalRejected,
    WrongTenant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProposalLifecycle {
    Proposed,
    Approved,
    Committed,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProposalRecord {
    id: String,
    input_fingerprint: String,
    evidence_refs: Vec<String>,
    idempotency_key: String,
    lifecycle: ProposalLifecycle,
}

struct HostHarness {
    service: TempliqxService<
        FilesystemPackageStore,
        FilesystemArtifactWorkspace,
        ScriptedRuntime,
        UnsupportedLegacyAdapter,
        UnsupportedDocumentRenderer,
    >,
    runtime: ScriptedRuntime,
    idempotency: BTreeMap<String, OperationEnvelope<ExecutionReceipt>>,
    proposals: BTreeMap<String, ProposalRecord>,
    document_queue: Vec<String>,
    audit_log: Vec<String>,
    retry_elapsed_ms: u64,
}

impl HostHarness {
    fn new(runtime: ScriptedRuntime) -> Result<Self> {
        let workspace = tempfile::tempdir()?.keep();
        Ok(Self {
            service: TempliqxService::new(
                FilesystemPackageStore::new(repo_root().join("examples"))?,
                FilesystemArtifactWorkspace::new(workspace)?,
                runtime.clone(),
                UnsupportedLegacyAdapter,
                UnsupportedDocumentRenderer,
            ),
            runtime,
            idempotency: BTreeMap::new(),
            proposals: BTreeMap::new(),
            document_queue: Vec::new(),
            audit_log: Vec::new(),
            retry_elapsed_ms: 0,
        })
    }

    fn execute(
        &mut self,
        actor: Actor,
        tenant: &str,
        approval_id: Option<&str>,
        idempotency_key: Option<&str>,
        max_attempts: usize,
    ) -> Result<OperationEnvelope<ExecutionReceipt>, HostRejection> {
        if tenant != TENANT {
            return Err(HostRejection::WrongTenant);
        }
        if actor == Actor::Agent && approval_id.is_none() {
            return Err(HostRejection::DirectAgentCommitDenied);
        }
        if actor == Actor::Agent
            && let Some(approval_id) = approval_id
            && let Some(proposal) = self.proposals.get(approval_id)
            && !matches!(
                proposal.lifecycle,
                ProposalLifecycle::Approved | ProposalLifecycle::Committed
            )
        {
            return Err(HostRejection::ProposalRejected);
        }
        if let Some(key) = idempotency_key
            && let Some(cached) = self.idempotency.get(key)
        {
            return Ok(cached.clone());
        }

        let capabilities = ["structured_output".to_string()];
        let request = request();
        let output = expected_output();
        let mut last = self.service.execute_contract(
            PACKAGE,
            CONTRACT,
            &request,
            &capabilities,
            Some(output.clone()),
        );

        for _ in 1..max_attempts {
            if !is_retryable(&last) {
                break;
            }
            self.retry_elapsed_ms = self.retry_elapsed_ms.saturating_add(retry_after_ms(&last));
            last = self.service.execute_contract(
                PACKAGE,
                CONTRACT,
                &request,
                &capabilities,
                Some(output.clone()),
            );
        }

        if is_retryable(&last) && self.runtime.stats().attempts >= max_attempts {
            let prior = last
                .diagnostics
                .first()
                .expect("retryable failure diagnostic");
            let prior_fingerprint = prior
                .help
                .as_deref()
                .and_then(|help| {
                    help.split_whitespace()
                        .find_map(|field| field.strip_prefix("fingerprint="))
                })
                .unwrap_or("none");
            last = OperationEnvelope::new(
                "execute_contract",
                None,
                vec![Diagnostic {
                    code: RuntimeFailureCode::HostRetryExhausted.as_str().into(),
                    severity: Severity::Error,
                    message: "host retry policy exhausted actual unavailable runtime attempts"
                        .into(),
                    file: None,
                    json_pointer: None,
                    span: None,
                    help: Some(format!(
                        "attempts={} prior_fingerprint={prior_fingerprint}",
                        self.runtime.stats().attempts
                    )),
                }],
            );
        }

        if let Some(key) = idempotency_key {
            self.idempotency.insert(key.into(), last.clone());
            if last.ok && approval_id.is_some() {
                self.document_queue.push(key.into());
                self.audit_log.push(key.into());
            }
        }
        Ok(last)
    }

    fn propose(&mut self, id: &str, idempotency_key: &str) -> ProposalRecord {
        let request = request();
        let record = ProposalRecord {
            id: id.into(),
            input_fingerprint: templiqx_contracts::fingerprint(&request.inputs).unwrap(),
            evidence_refs: vec!["SYN-DOC-0001#clause-2".into()],
            idempotency_key: idempotency_key.into(),
            lifecycle: ProposalLifecycle::Proposed,
        };
        self.proposals.insert(id.into(), record.clone());
        record
    }

    fn approve_proposal(&mut self, id: &str) -> Result<(), HostRejection> {
        let proposal = self
            .proposals
            .get_mut(id)
            .ok_or(HostRejection::DirectAgentCommitDenied)?;
        if proposal.lifecycle == ProposalLifecycle::Rejected {
            return Err(HostRejection::ProposalRejected);
        }
        if proposal.lifecycle == ProposalLifecycle::Proposed {
            proposal.lifecycle = ProposalLifecycle::Approved;
        }
        Ok(())
    }

    fn reject_proposal(&mut self, id: &str) -> Result<(), HostRejection> {
        let proposal = self
            .proposals
            .get_mut(id)
            .ok_or(HostRejection::DirectAgentCommitDenied)?;
        if proposal.lifecycle == ProposalLifecycle::Committed {
            return Err(HostRejection::ProposalRejected);
        }
        proposal.lifecycle = ProposalLifecycle::Rejected;
        Ok(())
    }

    fn commit_proposal(
        &mut self,
        id: &str,
    ) -> Result<OperationEnvelope<ExecutionReceipt>, HostRejection> {
        let state = self
            .proposals
            .get(id)
            .ok_or(HostRejection::DirectAgentCommitDenied)?;
        if state.lifecycle == ProposalLifecycle::Committed {
            return self
                .idempotency
                .get(&state.idempotency_key)
                .cloned()
                .ok_or(HostRejection::ProposalRejected);
        }
        self.approve_proposal(id)?;
        let idempotency_key = {
            let proposal = self
                .proposals
                .get(id)
                .ok_or(HostRejection::DirectAgentCommitDenied)?;
            if proposal.lifecycle != ProposalLifecycle::Approved {
                return Err(HostRejection::ProposalRejected);
            }
            proposal.idempotency_key.clone()
        };
        let result = self.execute(Actor::Agent, TENANT, Some(id), Some(&idempotency_key), 1)?;
        let proposal = self
            .proposals
            .get_mut(id)
            .expect("proposal remains present");
        if result.ok {
            proposal.lifecycle = ProposalLifecycle::Committed;
        }
        Ok(result)
    }

    fn attempts(&self) -> usize {
        self.runtime.stats().attempts
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root")
}

fn request() -> RenderRequest {
    serde_json::from_slice(
        &fs::read(repo_root().join("examples/crm3/evals/bli-61-request.json")).unwrap(),
    )
    .unwrap()
}

fn expected_output() -> serde_json::Value {
    serde_json::from_slice(
        &fs::read(repo_root().join("examples/crm3/evals/bli-61-output.json")).unwrap(),
    )
    .unwrap()
}

fn is_retryable(envelope: &OperationEnvelope<ExecutionReceipt>) -> bool {
    envelope.diagnostics.iter().any(|diagnostic| {
        matches!(
            diagnostic.code.as_str(),
            "TQX_RUNTIME_TIMEOUT" | "TQX_RUNTIME_RATE_LIMITED" | "TQX_RUNTIME_UNAVAILABLE"
        )
    })
}

fn retry_after_ms(envelope: &OperationEnvelope<ExecutionReceipt>) -> u64 {
    envelope
        .diagnostics
        .iter()
        .find_map(|diagnostic| diagnostic.help.as_deref())
        .and_then(|help| {
            help.split_whitespace().find_map(|field| {
                field
                    .strip_prefix("retry_after_ms=")
                    .and_then(|value| value.parse().ok())
            })
        })
        .unwrap_or(0)
}

fn ok_receipt(
    envelope: OperationEnvelope<ExecutionReceipt>,
) -> Result<OperationEnvelope<ExecutionReceipt>> {
    ensure!(
        envelope.ok,
        "expected ok envelope: {:?}",
        envelope.diagnostics
    );
    ensure!(envelope.result.is_some(), "expected receipt");
    Ok(envelope)
}

#[test]
fn human_and_approved_agent_have_actor_parity() -> Result<()> {
    let mut human = HostHarness::new(ScriptedRuntime::success())?;
    let mut agent = HostHarness::new(ScriptedRuntime::success())?;

    let human = ok_receipt(human.execute(Actor::Human, TENANT, None, None, 1).unwrap())?;
    let agent = ok_receipt(
        agent
            .execute(Actor::Agent, TENANT, Some("approval-1"), None, 1)
            .unwrap(),
    )?;

    ensure!(
        human.fingerprints == agent.fingerprints,
        "actor fingerprints diverged"
    );
    ensure!(
        human.result.context("human receipt")? == agent.result.context("agent receipt")?,
        "actor receipts diverged"
    );
    Ok(())
}

#[test]
fn host_denies_direct_agent_commit_before_runtime() -> Result<()> {
    let mut host = HostHarness::new(ScriptedRuntime::success())?;
    let rejected = host
        .execute(Actor::Agent, TENANT, None, None, 1)
        .expect_err("agent without approval should be denied");

    ensure!(rejected == HostRejection::DirectAgentCommitDenied);
    ensure!(host.attempts() == 0, "runtime should not be reached");
    ensure!(
        host.document_queue.is_empty() && host.audit_log.is_empty(),
        "rejected proposal created side effects"
    );
    Ok(())
}

#[test]
fn rejected_proposal_has_no_document_queue_or_audit_side_effect() -> Result<()> {
    let mut host = HostHarness::new(ScriptedRuntime::success())?;
    let proposal = host.propose("proposal-rejected", "idem-rejected");
    ensure!(proposal.lifecycle == ProposalLifecycle::Proposed);
    host.reject_proposal("proposal-rejected").unwrap();
    ensure!(host.proposals["proposal-rejected"].lifecycle == ProposalLifecycle::Rejected);
    ensure!(host.commit_proposal("proposal-rejected") == Err(HostRejection::ProposalRejected));
    ensure!(host.document_queue.is_empty() && host.audit_log.is_empty());
    ensure!(host.attempts() == 0, "rejected proposal reached runtime");
    Ok(())
}

#[test]
fn direct_agent_execute_rejects_rejected_proposal_before_idempotency_or_runtime() -> Result<()> {
    let mut host = HostHarness::new(ScriptedRuntime::success())?;
    host.propose("proposal-direct-rejected", "idem-direct-rejected");
    host.reject_proposal("proposal-direct-rejected").unwrap();

    let rejected = host
        .execute(
            Actor::Agent,
            TENANT,
            Some("proposal-direct-rejected"),
            Some("idem-direct-rejected"),
            1,
        )
        .expect_err("direct execute should reject a rejected proposal");

    ensure!(rejected == HostRejection::ProposalRejected);
    ensure!(host.attempts() == 0, "rejected proposal reached runtime");
    ensure!(host.idempotency.is_empty(), "rejected proposal was cached");
    ensure!(host.document_queue.is_empty() && host.audit_log.is_empty());
    Ok(())
}

#[test]
fn approved_proposal_commit_is_idempotent_and_has_one_side_effect() -> Result<()> {
    let mut host = HostHarness::new(ScriptedRuntime::success())?;
    let proposal = host.propose("proposal-approved", "idem-approved");
    ensure!(!proposal.input_fingerprint.is_empty() && !proposal.evidence_refs.is_empty());
    let first = ok_receipt(host.commit_proposal("proposal-approved").unwrap())?;
    let second = ok_receipt(host.commit_proposal("proposal-approved").unwrap())?;
    ensure!(first == second);
    ensure!(host.document_queue.len() == 1 && host.audit_log.len() == 1);
    ensure!(host.proposals["proposal-approved"].lifecycle == ProposalLifecycle::Committed);
    Ok(())
}

#[test]
fn approval_and_idempotency_are_host_policy() -> Result<()> {
    let mut host = HostHarness::new(ScriptedRuntime::success())?;

    let first = ok_receipt(
        host.execute(
            Actor::Agent,
            TENANT,
            Some("approval-1"),
            Some("idem-compile-1"),
            1,
        )
        .unwrap(),
    )?;
    let second = ok_receipt(
        host.execute(
            Actor::Agent,
            TENANT,
            Some("approval-1"),
            Some("idem-compile-1"),
            1,
        )
        .unwrap(),
    )?;

    ensure!(first == second, "idempotency replay changed envelope");
    ensure!(
        host.attempts() == 1,
        "idempotency replay re-entered runtime"
    );
    Ok(())
}

#[test]
fn wrong_tenant_is_rejected_before_runtime() -> Result<()> {
    let mut host = HostHarness::new(ScriptedRuntime::success())?;
    let rejected = host
        .execute(Actor::Human, "tenant-other", None, None, 1)
        .expect_err("wrong tenant should be rejected");

    ensure!(rejected == HostRejection::WrongTenant);
    ensure!(host.attempts() == 0, "runtime should not be reached");
    Ok(())
}

#[test]
fn retry_after_uses_virtual_elapsed_time_and_stops_at_max_attempts() -> Result<()> {
    let runtime = ScriptedRuntime::scripted([
        ScriptedScenario::Failure {
            id: "rate-limit-1".into(),
            code: RuntimeFailureCode::RateLimited,
            retry_after_ms: Some(10),
            detail: "rate limited".into(),
        },
        ScriptedScenario::Failure {
            id: "rate-limit-2".into(),
            code: RuntimeFailureCode::RateLimited,
            retry_after_ms: Some(20),
            detail: "still rate limited".into(),
        },
        ScriptedScenario::Success,
    ]);
    let mut host = HostHarness::new(runtime)?;

    let envelope = host
        .execute(Actor::Agent, TENANT, Some("approval-1"), None, 2)
        .unwrap();

    ensure!(
        !envelope.ok,
        "max attempts should stop before third success"
    );
    ensure!(host.attempts() == 2, "host exceeded max attempts");
    ensure!(
        host.retry_elapsed_ms == 10,
        "host should advance virtual retry clock once"
    );
    Ok(())
}

#[test]
fn permanent_failures_are_not_retried() -> Result<()> {
    let runtime = ScriptedRuntime::scripted([
        ScriptedScenario::Failure {
            id: "permanent".into(),
            code: RuntimeFailureCode::Permanent,
            retry_after_ms: Some(10),
            detail: "permanent failure".into(),
        },
        ScriptedScenario::Success,
    ]);
    let mut host = HostHarness::new(runtime)?;

    let envelope = host
        .execute(Actor::Agent, TENANT, Some("approval-1"), None, 3)
        .unwrap();

    ensure!(!envelope.ok, "permanent failure should remain failed");
    ensure!(host.attempts() == 1, "permanent failure was retried");
    ensure!(
        host.retry_elapsed_ms == 0,
        "permanent failure should not advance retry clock"
    );
    Ok(())
}

#[test]
fn actual_unavailable_attempts_end_in_host_retry_exhausted_without_effects() -> Result<()> {
    let runtime = ScriptedRuntime::scripted([
        ScriptedScenario::Failure {
            id: "unavailable-1".into(),
            code: RuntimeFailureCode::Unavailable,
            retry_after_ms: Some(1),
            detail: "down".into(),
        },
        ScriptedScenario::Failure {
            id: "unavailable-2".into(),
            code: RuntimeFailureCode::Unavailable,
            retry_after_ms: Some(1),
            detail: "still down".into(),
        },
        ScriptedScenario::Failure {
            id: "unavailable-3".into(),
            code: RuntimeFailureCode::Unavailable,
            retry_after_ms: Some(1),
            detail: "still down".into(),
        },
    ]);
    let mut host = HostHarness::new(runtime)?;
    let result = host
        .execute(
            Actor::Agent,
            TENANT,
            Some("approval-retry"),
            Some("idem-retry"),
            3,
        )
        .unwrap();
    ensure!(!result.ok && result.result.is_none());
    ensure!(
        result
            .diagnostics
            .iter()
            .any(|d| d.code == "TQX_HOST_RETRY_EXHAUSTED")
    );
    ensure!(host.attempts() == 3);
    ensure!(host.document_queue.is_empty() && host.audit_log.is_empty());
    Ok(())
}
