import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import fs from "node:fs/promises";
import { createServer } from "node:net";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { afterAll, beforeAll, describe, expect, it } from "vitest";

import {
  TempliqxHttpError,
  TempliqxTransportError,
  createTempliqxClient,
  type TempliqxClient,
} from "../src/index.js";

const enabled = process.env.TEMPLIQX_SDK_IT === "1";
const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../..");
const fingerprintPattern = /^(?:sha256:)?[a-f0-9]{64}$/;

async function freePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const server = createServer();
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      if (!address || typeof address === "string") {
        server.close();
        reject(new Error("Could not reserve a TCP port"));
        return;
      }
      server.close((error) => (error ? reject(error) : resolve(address.port)));
    });
  });
}

describe.skipIf(!enabled)("Templiqx SDK against the deterministic-fake HTTP server", () => {
  let server: ChildProcessWithoutNullStreams;
  let tempDir: string;
  let baseUrl: string;
  let client: TempliqxClient;
  let serverOutput = "";

  beforeAll(async () => {
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), "templiqx-sdk-it-"));
    const packagesRoot = path.join(tempDir, "packages");
    const workspaceRoot = path.join(tempDir, "workspace");
    await fs.mkdir(packagesRoot, { recursive: true });
    await fs.mkdir(workspaceRoot, { recursive: true });
    const port = await freePort();
    baseUrl = `http://127.0.0.1:${port}`;
    const { MODEL_API_KEY: _modelApiKey, ...environment } = process.env;

    server = spawn("cargo", ["run", "--quiet", "-p", "templiqx-http-server"], {
      cwd: repoRoot,
      env: {
        ...environment,
        TEMPLIQX_HTTP_ADDR: `127.0.0.1:${port}`,
        TEMPLIQX_ROOT: packagesRoot,
        TEMPLIQX_WORKSPACE: workspaceRoot,
      },
      stdio: ["pipe", "pipe", "pipe"],
    });
    server.stdout.on("data", (chunk) => (serverOutput += chunk.toString()));
    server.stderr.on("data", (chunk) => (serverOutput += chunk.toString()));

    const deadline = Date.now() + 120_000;
    while (Date.now() < deadline) {
      if (server.exitCode !== null) throw new Error(`Server exited during startup:\n${serverOutput}`);
      try {
        const response = await fetch(`${baseUrl}/operations/v1/health/ready`);
        if (response.ok) break;
      } catch {
        // The server is still compiling or binding its listener.
      }
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
    const ready = await fetch(`${baseUrl}/operations/v1/health/ready`).catch(() => undefined);
    if (!ready?.ok) throw new Error(`Server did not become ready:\n${serverOutput}`);
    client = createTempliqxClient({ baseUrl, timeoutMs: 5_000 });
  }, 130_000);

  afterAll(async () => {
    if (server && server.exitCode === null) {
      server.kill("SIGTERM");
      await new Promise<void>((resolve) => {
        server.once("exit", () => resolve());
        setTimeout(resolve, 5_000);
      });
    }
    if (tempDir) await fs.rm(tempDir, { recursive: true, force: true });
  });

  it("drives health, catalog, CAS, compile, execute, HTTP error, and abort paths", async () => {
    const live = await client.getOperationsV1Liveness();
    const ready = await client.getOperationsV1Readiness();
    expect(live.data.status).toBe("ok");
    expect(ready.data.status).toBe("ready");

    const catalog = await client.catalog();
    expect(catalog.data.ok).toBe(true);
    expect(catalog.data.result).toContain("execute_contract");

    const created = await client.createPackage({ name: "sdk-it", version: "0.1.0" });
    const packageFingerprint = created.data.fingerprints.package;
    expect(packageFingerprint).toMatch(fingerprintPattern);
    const updated = await client.updatePackage(
      { package: "sdk-it" },
      { description: "TypeScript SDK integration" },
      { ifMatch: packageFingerprint! },
    );
    expect(updated.data.ok).toBe(true);

    const contractSource = await fs.readFile(
      path.join(repoRoot, "examples/packages/demo/contracts/greeting.yaml"),
      "utf8",
    );
    const put = await client.putContract(
      { package: "sdk-it", contract: "greeting" },
      contractSource,
    );
    expect(put.data.ok).toBe(true);

    const render = {
      inputs: { name: "Ryan" },
      context: { organization: "Blinqx" },
    };
    const compile = await client.compileContract(
      { package: "sdk-it", contract: "greeting" },
      { render, capabilities: ["structured_output"] },
    );
    expect(compile.data.ok).toBe(true);

    const execution = await client.executeContract(
      { package: "sdk-it", contract: "greeting" },
      {
        render,
        capabilities: ["structured_output"],
        fixture_output: { greeting: "Hello Ryan" },
        stream: false,
      },
    );
    expect(execution.data.ok).toBe(true);
    expect(execution.data.result?.output_fingerprint).toMatch(fingerprintPattern);
    console.log(`ExecutionReceipt fingerprint: ${execution.data.result!.output_fingerprint}`);

    const httpError = await client
      .inspectContract({ package: "missing", contract: "greeting" })
      .catch((error: unknown) => error);
    expect(httpError).toBeInstanceOf(TempliqxHttpError);
    expect(httpError).toMatchObject({ status: 404 });
    expect((httpError as TempliqxHttpError).envelope?.diagnostics[0]?.code).toBe("TQX_NOT_FOUND");

    const controller = new AbortController();
    controller.abort(new Error("integration cancellation"));
    await expect(
      client.catalog({ signal: controller.signal, requestId: "sdk-it-abort" }),
    ).rejects.toBeInstanceOf(TempliqxTransportError);
  }, 30_000);
});
