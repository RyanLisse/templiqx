import { describe, expect, it, vi } from "vitest";

import {
  TempliqxHttpError,
  TempliqxTransportError,
  compatibility,
  createTempliqxClient,
} from "../src/index.js";
import type { components } from "../src/index.js";

function jsonResponse(body: unknown, init: ResponseInit = {}): Response {
  const headers = new Headers(init.headers);
  headers.set("content-type", "application/json");
  return new Response(JSON.stringify(body), { ...init, headers });
}

describe("createTempliqxClient", () => {
  it("builds typed operation requests and returns the effective request id", async () => {
    const fetchStub = vi.fn(async (_input: RequestInfo | URL, init?: RequestInit) => {
      const headers = new Headers(init?.headers);
      expect(headers.get("x-request-id")).toBe("sdk-request-42");
      expect(headers.get("x-tenant-id")).toBe("tenant-a");
      return jsonResponse(
        {
          api_version: "templiqx/v1alpha1",
          operation: "compile_contract",
          ok: true,
          diagnostics: [],
          fingerprints: {},
          result: { contract_id: "greeting", messages: [], output_schema: {}, required_capabilities: [] },
        },
        { headers: { "x-request-id": "sdk-request-42" } },
      );
    });
    const client = createTempliqxClient({
      baseUrl: "https://templiqx.example/",
      fetch: fetchStub as typeof fetch,
      defaultHeaders: { "x-tenant-id": "tenant-a" },
    });

    const response = await client.compileContract(
      { package: "demo package", contract: "greeting" },
      { render: { inputs: { name: "Ryan" } }, capabilities: ["structured_output"] },
      { requestId: "sdk-request-42" },
    );

    expect(response.requestId).toBe("sdk-request-42");
    expect(fetchStub).toHaveBeenCalledOnce();
    const [url, init] = fetchStub.mock.calls[0]!;
    expect(url).toBe(
      "https://templiqx.example/operations/v1/packages/demo%20package/contracts/greeting/compile",
    );
    expect(init?.method).toBe("POST");
    expect(JSON.parse(String(init?.body))).toEqual({
      render: { inputs: { name: "Ryan" } },
      capabilities: ["structured_output"],
    });
  });

  it("sends If-Match only for a CAS operation", async () => {
    const seenHeaders: Headers[] = [];
    const fetchStub = vi.fn(async (_input: RequestInfo | URL, init?: RequestInit) => {
      seenHeaders.push(new Headers(init?.headers));
      return jsonResponse({
        api_version: "templiqx/v1alpha1",
        operation: "operation",
        ok: true,
        diagnostics: [],
        fingerprints: {},
      });
    });
    const client = createTempliqxClient({ baseUrl: "https://example.test", fetch: fetchStub as typeof fetch });

    await client.catalog({ ifMatch: "ignored" });
    await client.updatePackage(
      { package: "demo" },
      { version: "0.2.0" },
      { ifMatch: "sha256:abc", requestId: "cas-request" },
    );

    expect(seenHeaders[0]!.has("if-match")).toBe(false);
    expect(seenHeaders[1]!.get("if-match")).toBe("sha256:abc");
  });

  it("posts typed quality evidence to the package-scoped assessment route", async () => {
    const fetchStub = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      expect(input).toBe(
        "https://example.test/operations/v1/packages/demo%20package/quality/proposals:assess",
      );
      expect(init?.method).toBe("POST");
      expect(JSON.parse(String(init?.body))).toMatchObject({
        package: "demo package",
        contract_id: "greeting",
        candidates: [],
      });
      return jsonResponse({
        api_version: "templiqx/v1alpha1",
        operation: "assess_quality_proposals",
        ok: true,
        diagnostics: [],
        fingerprints: {},
      });
    });
    const client = createTempliqxClient({
      baseUrl: "https://example.test",
      fetch: fetchStub as typeof fetch,
    });
    const fingerprint = "a".repeat(64);

    await client.assessQualityProposals(
      { package: "demo package" },
      {
        package: "demo package",
        contract_id: "greeting",
        expected_package_fingerprint: fingerprint,
        expected_base_contract_fingerprint: fingerprint,
        expected_fixture_set_fingerprint: fingerprint,
        policy: {
          id: "quality-policy",
          replicates_per_fixture: 1,
          minimum_semantic_cases: 1,
          maximum_infrastructure_failure_ppm: 0,
          claimed_evaluator_profile_fingerprint: fingerprint,
          claimed_model_profile_fingerprint: fingerprint,
          binary_scorers: [],
          objectives: [],
          eligibility_rules: [],
        },
        candidates: [],
      },
    );
  });

  it("keeps quality integers exact at the public ceiling and exposes claimed output names", () => {
    const fingerprint = "a".repeat(64);
    const observation: components["schemas"]["MetricObservation"] = {
      metric_id: "total_tokens",
      unit: "token_count",
      value: Number.MAX_SAFE_INTEGER,
      claimed_measurement_profile_fingerprint: fingerprint,
      token_kind: "total",
    };
    const identities: components["schemas"]["ClaimedQualityIdentities"] = {
      claimed_candidate_contract_fingerprint: fingerprint,
      claimed_evaluator_profile_fingerprint: fingerprint,
      claimed_model_profile_fingerprint: fingerprint,
      claimed_scorer_fingerprints: { grounded: fingerprint },
      claimed_measurement_profile_fingerprints: { total_tokens: fingerprint },
    };
    const invalidAssessment: components["schemas"]["CandidateAssessment"] = {
      eligibility: {
        eligible: false,
        total_trial_count: 0,
        semantic_trial_count: 0,
        infrastructure_trial_count: 0,
        semantic_coverage_ppm: 0,
        infrastructure_failure_ppm: 0,
        gates: [],
      },
      aggregates: [],
      trial_summaries: [],
      proposal_change_paths: [],
      diagnostics: [],
    };

    expect(Number.isSafeInteger(observation.value)).toBe(true);
    expect(JSON.parse(JSON.stringify(observation)).value).toBe(Number.MAX_SAFE_INTEGER);
    expect(Object.keys(identities).sort()).toEqual([
      "claimed_candidate_contract_fingerprint",
      "claimed_evaluator_profile_fingerprint",
      "claimed_measurement_profile_fingerprints",
      "claimed_model_profile_fingerprint",
      "claimed_scorer_fingerprints",
    ]);
    expect(invalidAssessment).not.toHaveProperty("claimed_identities");
  });

  it("maps JSON operation envelopes on non-2xx responses", async () => {
    const envelope = {
      api_version: "templiqx/v1alpha1" as const,
      operation: "inspect_contract",
      ok: false,
      diagnostics: [{ code: "TQX_NOT_FOUND", severity: "error" as const, message: "missing" }],
      fingerprints: {},
    };
    const client = createTempliqxClient({
      baseUrl: "https://example.test",
      fetch: vi.fn(async () => jsonResponse(envelope, { status: 404 })) as typeof fetch,
    });

    const error = await client
      .inspectContract({ package: "missing", contract: "greeting" })
      .catch((cause: unknown) => cause);

    expect(error).toBeInstanceOf(TempliqxHttpError);
    expect(error).toMatchObject({ status: 404, envelope, rawBody: undefined });
  });

  it("preserves a raw non-envelope HTTP error body", async () => {
    const client = createTempliqxClient({
      baseUrl: "https://example.test",
      fetch: vi.fn(async () => new Response("gateway unavailable", { status: 502 })) as typeof fetch,
    });

    const error = await client.catalog({ requestId: "raw-error" }).catch((cause: unknown) => cause);

    expect(error).toBeInstanceOf(TempliqxHttpError);
    expect(error).toMatchObject({ status: 502, rawBody: "gateway unavailable", requestId: "raw-error" });
  });

  it("maps network failures and caller cancellation to transport errors", async () => {
    const networkCause = new TypeError("network down");
    const networkClient = createTempliqxClient({
      baseUrl: "https://example.test",
      fetch: vi.fn(async () => Promise.reject(networkCause)) as typeof fetch,
    });
    const networkError = await networkClient.catalog({ requestId: "network-request" }).catch((cause: unknown) => cause);
    expect(networkError).toBeInstanceOf(TempliqxTransportError);
    expect(networkError).toMatchObject({ requestId: "network-request", cause: networkCause });

    const waitingFetch = vi.fn((_input: RequestInfo | URL, init?: RequestInit) =>
      new Promise<Response>((_resolve, reject) => {
        const rejectAbort = () => reject(init?.signal?.reason);
        if (init?.signal?.aborted) rejectAbort();
        else init?.signal?.addEventListener("abort", rejectAbort, { once: true });
      }),
    );
    const abortClient = createTempliqxClient({
      baseUrl: "https://example.test",
      fetch: waitingFetch as typeof fetch,
      timeoutMs: 5,
    });
    const timeoutError = await abortClient.catalog({ requestId: "timeout-request" }).catch((cause: unknown) => cause);
    expect(timeoutError).toBeInstanceOf(TempliqxTransportError);
    expect(timeoutError).toMatchObject({ requestId: "timeout-request" });

    const controller = new AbortController();
    controller.abort(new Error("caller stopped"));
    const abortError = await abortClient
      .catalog({ requestId: "abort-request", signal: controller.signal, timeoutMs: 1_000 })
      .catch((cause: unknown) => cause);
    expect(abortError).toBeInstanceOf(TempliqxTransportError);
    expect(abortError).toMatchObject({ requestId: "abort-request" });
  });

  it("exposes all 32 OpenAPI operationId methods and generated compatibility metadata", () => {
    const client = createTempliqxClient({ baseUrl: "https://example.test", fetch: vi.fn() as typeof fetch });
    const operationIds = [
      "getOperationsV1Liveness",
      "getOperationsV1Readiness",
      "getOperationsV1OpenApiYaml",
      "getOperationsV1OpenApi",
      "catalog",
      "discoverPackages",
      "createPackage",
      "inspectContract",
      "putContract",
      "deleteContract",
      "validateContract",
      "compileContract",
      "executeContract",
      "updatePackage",
      "deletePackage",
      "validatePackage",
      "testPackage",
      "exportPackageIdentity",
      "signPackage",
      "verifyPackageTrust",
      "listEvals",
      "runEval",
      "assessQualityProposals",
      "renderContract",
      "diffContract",
      "explainContract",
      "migrateLegacy",
      "renderDocument",
      "inspectDocument",
      "listWorkspaceArtifacts",
      "readArtifact",
      "deleteWorkspaceArtifact",
    ] as const;

    expect(operationIds).toHaveLength(32);
    for (const operationId of operationIds) expect(client[operationId]).toBeTypeOf("function");
    expect(compatibility.opsApiVersion).toBe("1.0.0-alpha.2");
    expect(compatibility.openApiDigest).toMatch(/^sha256:[a-f0-9]{64}$/);
    expect(compatibility.engineApiVersion).toBe("0.2");
    expect(compatibility.engineVersion).toBe("0.2.0");
    expect(compatibility.contractFormat).toBe("templiqx/v1alpha1");
  });
});
