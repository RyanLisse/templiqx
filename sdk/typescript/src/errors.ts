import type { components } from "./generated/operations-v1.js";

export type OperationEnvelope = components["schemas"]["OperationEnvelopeBase"];

export class TempliqxTransportError extends Error {
  readonly requestId: string;
  readonly cause: unknown;

  constructor(requestId: string, cause: unknown) {
    super("Templiqx request failed before receiving an HTTP response", { cause });
    this.name = "TempliqxTransportError";
    this.requestId = requestId;
    this.cause = cause;
  }
}

export class TempliqxHttpError extends Error {
  readonly status: number;
  readonly envelope?: OperationEnvelope;
  readonly rawBody?: string;
  readonly requestId: string;

  constructor(options: {
    status: number;
    envelope?: OperationEnvelope;
    rawBody?: string;
    requestId: string;
  }) {
    super(`Templiqx request failed with HTTP ${options.status}`);
    this.name = "TempliqxHttpError";
    this.status = options.status;
    this.envelope = options.envelope;
    this.rawBody = options.rawBody;
    this.requestId = options.requestId;
  }
}
