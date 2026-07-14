# @blinqx/templiqx-adapter

Thin TypeScript transport client for the Templiqx Operations API.

```sh
npm install @blinqx/templiqx-adapter
```

```ts
import {
  createTempliqxClient,
  TempliqxHttpError,
  TempliqxTransportError,
} from "@blinqx/templiqx-adapter";

const templiqx = createTempliqxClient({
  baseUrl: "http://localhost:8080",
  timeoutMs: 30_000,
});

try {
  const { data, requestId } = await templiqx.executeContract(
    { package: "demo", contract: "greeting" },
    {
      render: { inputs: { name: "Ryan" }, context: { organization: "Blinqx" } },
      capabilities: ["structured_output"],
      fixture_output: { greeting: "Hello Ryan" }, // deterministic-fake mode
    },
  );
  console.log(requestId, data.result?.output_fingerprint);
} catch (error) {
  if (error instanceof TempliqxHttpError) console.error(error.envelope?.diagnostics);
  else if (error instanceof TempliqxTransportError) console.error(error.requestId, error.cause);
  else throw error;
}
```

Regenerate checked-in DTOs after changing the wire contract:

```sh
npm run generate
npm run generate:check
```

The SDK contains transport ergonomics only. Templiqx owns contract semantics,
diagnostics, fingerprints, capability checks, and CAS enforcement.
