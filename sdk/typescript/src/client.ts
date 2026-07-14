import type { operations } from "./generated/operations-v1.js";
import {
  TempliqxHttpError,
  TempliqxTransportError,
  type OperationEnvelope,
} from "./errors.js";

type OperationId = keyof operations;
type Operation<K extends OperationId> = operations[K];
type ContentValue<C> = C extends Record<PropertyKey, unknown> ? C[keyof C] : never;
type OperationParameters<
  K extends OperationId,
  Position extends "path" | "query",
> = Operation<K> extends { parameters: infer P }
  ? Position extends keyof P
    ? NonNullable<P[Position]>
    : never
  : never;
type OperationBody<K extends OperationId> = Operation<K> extends {
  requestBody: { content: infer C };
}
  ? ContentValue<C>
  : never;
type ResponseContent<R> = R extends { content: infer C } ? ContentValue<C> : never;
type ResponseAt<R, Status extends number> = Status extends keyof R
  ? ResponseContent<R[Status]>
  : never;
type OperationResult<K extends OperationId> = Operation<K> extends { responses: infer R }
  ? ResponseAt<R, 200> | ResponseAt<R, 201> | ResponseAt<R, 202>
  : never;

export interface TempliqxResponse<T> {
  data: T;
  requestId: string;
}

export interface CallOptions {
  signal?: AbortSignal;
  timeoutMs?: number;
  requestId?: string;
  ifMatch?: string;
}

export type CasCallOptions = CallOptions & { ifMatch: string };

export interface CreateTempliqxClientOptions {
  baseUrl: string;
  fetch?: typeof globalThis.fetch;
  timeoutMs?: number;
  defaultHeaders?: HeadersInit;
}

type Result<K extends OperationId> = Promise<TempliqxResponse<OperationResult<K>>>;

export interface TempliqxClient {
  getOperationsV1Liveness(options?: CallOptions): Result<"getOperationsV1Liveness">;
  getOperationsV1Readiness(options?: CallOptions): Result<"getOperationsV1Readiness">;
  getOperationsV1OpenApiYaml(options?: CallOptions): Result<"getOperationsV1OpenApiYaml">;
  getOperationsV1OpenApi(options?: CallOptions): Result<"getOperationsV1OpenApi">;
  catalog(options?: CallOptions): Result<"catalog">;
  discoverPackages(options?: CallOptions): Result<"discoverPackages">;
  createPackage(body: OperationBody<"createPackage">, options?: CallOptions): Result<"createPackage">;
  inspectContract(
    path: OperationParameters<"inspectContract", "path">,
    options?: CallOptions,
  ): Result<"inspectContract">;
  putContract(
    path: OperationParameters<"putContract", "path">,
    body: OperationBody<"putContract">,
    options?: CallOptions,
  ): Result<"putContract">;
  deleteContract(
    path: OperationParameters<"deleteContract", "path">,
    options: CasCallOptions,
  ): Result<"deleteContract">;
  validateContract(
    path: OperationParameters<"validateContract", "path">,
    options?: CallOptions,
  ): Result<"validateContract">;
  compileContract(
    path: OperationParameters<"compileContract", "path">,
    body: OperationBody<"compileContract">,
    options?: CallOptions,
  ): Result<"compileContract">;
  executeContract(
    path: OperationParameters<"executeContract", "path">,
    body: OperationBody<"executeContract">,
    options?: CallOptions,
  ): Result<"executeContract">;
  updatePackage(
    path: OperationParameters<"updatePackage", "path">,
    body: OperationBody<"updatePackage">,
    options: CasCallOptions,
  ): Result<"updatePackage">;
  deletePackage(
    path: OperationParameters<"deletePackage", "path">,
    options: CasCallOptions,
  ): Result<"deletePackage">;
  validatePackage(
    path: OperationParameters<"validatePackage", "path">,
    options?: CallOptions,
  ): Result<"validatePackage">;
  testPackage(
    path: OperationParameters<"testPackage", "path">,
    body: OperationBody<"testPackage">,
    options?: CallOptions,
  ): Result<"testPackage">;
  exportPackageIdentity(
    path: OperationParameters<"exportPackageIdentity", "path">,
    options?: CallOptions,
  ): Result<"exportPackageIdentity">;
  signPackage(
    path: OperationParameters<"signPackage", "path">,
    body: OperationBody<"signPackage">,
    options: CasCallOptions,
  ): Result<"signPackage">;
  verifyPackageTrust(
    path: OperationParameters<"verifyPackageTrust", "path">,
    body: OperationBody<"verifyPackageTrust">,
    options?: CallOptions,
  ): Result<"verifyPackageTrust">;
  listEvals(
    path: OperationParameters<"listEvals", "path">,
    options?: CallOptions,
  ): Result<"listEvals">;
  runEval(
    path: OperationParameters<"runEval", "path">,
    body: OperationBody<"runEval">,
    options?: CallOptions,
  ): Result<"runEval">;
  renderContract(
    path: OperationParameters<"renderContract", "path">,
    body: OperationBody<"renderContract">,
    options?: CallOptions,
  ): Result<"renderContract">;
  diffContract(
    path: OperationParameters<"diffContract", "path">,
    body: OperationBody<"diffContract">,
    options?: CallOptions,
  ): Result<"diffContract">;
  explainContract(
    path: OperationParameters<"explainContract", "path">,
    options?: CallOptions,
  ): Result<"explainContract">;
  migrateLegacy(body: OperationBody<"migrateLegacy">, options?: CallOptions): Result<"migrateLegacy">;
  renderDocument(body: OperationBody<"renderDocument">, options?: CallOptions): Result<"renderDocument">;
  listWorkspaceArtifacts(
    query: OperationParameters<"listWorkspaceArtifacts", "query">,
    options?: CallOptions,
  ): Result<"listWorkspaceArtifacts">;
  readArtifact(
    path: OperationParameters<"readArtifact", "path">,
    query: OperationParameters<"readArtifact", "query">,
    options?: CallOptions,
  ): Result<"readArtifact">;
  deleteWorkspaceArtifact(
    path: OperationParameters<"deleteWorkspaceArtifact", "path">,
    query: OperationParameters<"deleteWorkspaceArtifact", "query">,
    options: CasCallOptions,
  ): Result<"deleteWorkspaceArtifact">;
}

type SameMembers<Left, Right> = [Exclude<Left, Right>, Exclude<Right, Left>] extends [
  never,
  never,
]
  ? true
  : false;
type Assert<T extends true> = T;
type OperationMethodCoverage = Assert<SameMembers<OperationId, keyof TempliqxClient>>;

interface DispatchRequest {
  method: string;
  path: string;
  options?: CallOptions;
  body?: unknown;
  contentType?: string;
  cas?: boolean;
}

const jsonContentType = "application/json";

function segment(value: string): string {
  return encodeURIComponent(value);
}

function artifactPath(value: string): string {
  return value.split("/").map(segment).join("/");
}

function withQuery(path: string, query: Record<string, unknown>): string {
  const search = new URLSearchParams();
  for (const [key, value] of Object.entries(query)) {
    if (value !== undefined) search.set(key, String(value));
  }
  const encoded = search.toString();
  return encoded ? `${path}?${encoded}` : path;
}

function operationEnvelope(value: unknown): OperationEnvelope | undefined {
  if (
    typeof value === "object" &&
    value !== null &&
    "diagnostics" in value &&
    Array.isArray(value.diagnostics)
  ) {
    return value as OperationEnvelope;
  }
  return undefined;
}

export function createTempliqxClient(config: CreateTempliqxClientOptions): TempliqxClient {
  const baseUrl = config.baseUrl.replace(/\/+$/, "");
  const fetchImpl = config.fetch ?? globalThis.fetch.bind(globalThis);
  const defaultHeaders = new Headers(config.defaultHeaders);
  const defaultTimeoutMs = config.timeoutMs ?? 30_000;

  async function dispatch<K extends OperationId>(
    request: DispatchRequest,
  ): Promise<TempliqxResponse<OperationResult<K>>> {
    const requestId = request.options?.requestId ?? globalThis.crypto.randomUUID();
    const headers = new Headers(defaultHeaders);
    headers.set("accept", "application/json, application/yaml");
    headers.set("x-request-id", requestId);
    if (request.body !== undefined) {
      headers.set("content-type", request.contentType ?? jsonContentType);
    }
    if (request.cas && request.options?.ifMatch) {
      headers.set("if-match", request.options.ifMatch);
    }

    const timeoutSignal = AbortSignal.timeout(request.options?.timeoutMs ?? defaultTimeoutMs);
    const signal = request.options?.signal
      ? AbortSignal.any([request.options.signal, timeoutSignal])
      : timeoutSignal;
    let response: Response;
    try {
      response = await fetchImpl(`${baseUrl}${request.path}`, {
        method: request.method,
        headers,
        body:
          request.body === undefined
            ? undefined
            : request.contentType === "application/yaml"
              ? String(request.body)
              : JSON.stringify(request.body),
        signal,
      });
    } catch (cause) {
      throw new TempliqxTransportError(requestId, cause);
    }

    const effectiveRequestId = response.headers.get("x-request-id") ?? requestId;
    if (!response.ok) {
      const rawBody = await response.text();
      let envelope: OperationEnvelope | undefined;
      try {
        envelope = operationEnvelope(JSON.parse(rawBody));
      } catch {
        // Preserve non-JSON response bodies verbatim for transport diagnosis.
      }
      throw new TempliqxHttpError({
        status: response.status,
        envelope,
        rawBody: envelope ? undefined : rawBody,
        requestId: effectiveRequestId,
      });
    }

    const contentType = response.headers.get("content-type") ?? "";
    const data = contentType.includes("json")
      ? await response.json()
      : await response.text();
    return { data: data as OperationResult<K>, requestId: effectiveRequestId };
  }

  const packagePath = (value: string) => `/operations/v1/packages/${segment(value)}`;
  const contractPath = (packageName: string, contract: string) =>
    `${packagePath(packageName)}/contracts/${segment(contract)}`;

  return {
    getOperationsV1Liveness: (options) =>
      dispatch<"getOperationsV1Liveness">({ method: "GET", path: "/operations/v1/health/live", options }),
    getOperationsV1Readiness: (options) =>
      dispatch<"getOperationsV1Readiness">({ method: "GET", path: "/operations/v1/health/ready", options }),
    getOperationsV1OpenApiYaml: (options) =>
      dispatch<"getOperationsV1OpenApiYaml">({ method: "GET", path: "/operations/v1/openapi.yaml", options }),
    getOperationsV1OpenApi: (options) =>
      dispatch<"getOperationsV1OpenApi">({ method: "GET", path: "/operations/v1/openapi.json", options }),
    catalog: (options) => dispatch<"catalog">({ method: "GET", path: "/operations/v1/catalog", options }),
    discoverPackages: (options) =>
      dispatch<"discoverPackages">({ method: "GET", path: "/operations/v1/packages", options }),
    createPackage: (body, options) =>
      dispatch<"createPackage">({ method: "POST", path: "/operations/v1/packages", body, options }),
    inspectContract: (path, options) =>
      dispatch<"inspectContract">({ method: "GET", path: contractPath(path.package, path.contract), options }),
    putContract: (path, body, options) =>
      dispatch<"putContract">({
        method: "PUT",
        path: contractPath(path.package, path.contract),
        body,
        contentType: "application/yaml",
        options,
        cas: true,
      }),
    deleteContract: (path, options) =>
      dispatch<"deleteContract">({
        method: "DELETE",
        path: contractPath(path.package, path.contract),
        options,
        cas: true,
      }),
    validateContract: (path, options) =>
      dispatch<"validateContract">({
        method: "POST",
        path: `${contractPath(path.package, path.contract)}/validate`,
        options,
      }),
    compileContract: (path, body, options) =>
      dispatch<"compileContract">({
        method: "POST",
        path: `${contractPath(path.package, path.contract)}/compile`,
        body,
        options,
      }),
    executeContract: (path, body, options) =>
      dispatch<"executeContract">({
        method: "POST",
        path: `${contractPath(path.package, path.contract)}/execute`,
        body,
        options,
      }),
    updatePackage: (path, body, options) =>
      dispatch<"updatePackage">({
        method: "PATCH",
        path: packagePath(path.package),
        body,
        options,
        cas: true,
      }),
    deletePackage: (path, options) =>
      dispatch<"deletePackage">({ method: "DELETE", path: packagePath(path.package), options, cas: true }),
    validatePackage: (path, options) =>
      dispatch<"validatePackage">({ method: "POST", path: `${packagePath(path.package)}/validate`, options }),
    testPackage: (path, body, options) =>
      dispatch<"testPackage">({ method: "POST", path: `${packagePath(path.package)}/test`, body, options }),
    exportPackageIdentity: (path, options) =>
      dispatch<"exportPackageIdentity">({ method: "GET", path: `${packagePath(path.package)}/identity`, options }),
    signPackage: (path, body, options) =>
      dispatch<"signPackage">({
        method: "POST",
        path: `${packagePath(path.package)}/sign`,
        body,
        options,
        cas: true,
      }),
    verifyPackageTrust: (path, body, options) =>
      dispatch<"verifyPackageTrust">({
        method: "POST",
        path: `${packagePath(path.package)}/verify-trust`,
        body,
        options,
      }),
    listEvals: (path, options) =>
      dispatch<"listEvals">({ method: "GET", path: `${packagePath(path.package)}/evals`, options }),
    runEval: (path, body, options) =>
      dispatch<"runEval">({ method: "POST", path: `${packagePath(path.package)}/evals/run`, body, options }),
    renderContract: (path, body, options) =>
      dispatch<"renderContract">({
        method: "POST",
        path: `${contractPath(path.package, path.contract)}/render`,
        body,
        options,
      }),
    diffContract: (path, body, options) =>
      dispatch<"diffContract">({
        method: "POST",
        path: `${contractPath(path.package, path.contract)}/diff`,
        body,
        options,
      }),
    explainContract: (path, options) =>
      dispatch<"explainContract">({
        method: "GET",
        path: `${contractPath(path.package, path.contract)}/explain`,
        options,
      }),
    migrateLegacy: (body, options) =>
      dispatch<"migrateLegacy">({ method: "POST", path: "/operations/v1/legacy/migrate", body, options }),
    renderDocument: (body, options) =>
      dispatch<"renderDocument">({ method: "POST", path: "/operations/v1/documents/render", body, options }),
    listWorkspaceArtifacts: (query, options) =>
      dispatch<"listWorkspaceArtifacts">({
        method: "GET",
        path: withQuery("/operations/v1/artifacts", query),
        options,
      }),
    readArtifact: (path, query, options) =>
      dispatch<"readArtifact">({
        method: "GET",
        path: withQuery(`/operations/v1/artifacts/${artifactPath(path.artifact)}`, query),
        options,
      }),
    deleteWorkspaceArtifact: (path, query, options) =>
      dispatch<"deleteWorkspaceArtifact">({
        method: "DELETE",
        path: withQuery(`/operations/v1/artifacts/${artifactPath(path.artifact)}`, query),
        options,
        cas: true,
      }),
  };
}
