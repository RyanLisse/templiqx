# Security policy

## Supported versions

Templiqx is currently pre-1.0. Security fixes are applied to the latest tagged
release and the `main` branch. Older pre-1.0 releases are not maintained.

## Reporting a vulnerability

Do not open a public issue for suspected vulnerabilities or customer-data
exposure. Use GitHub's private vulnerability reporting for this repository.
Include affected versions, reproduction steps, impact, and any suggested
mitigation. Maintainers will acknowledge a complete report within five business
days and coordinate disclosure after a fix is available.

## Security boundaries

Templiqx validates and compiles provider-neutral contracts. Authentication,
tenant authorization, retrieval, approval, audit persistence, provider secrets,
and production promotion are host responsibilities. The checked-in mock gateway
and synthetic fixtures are conformance-only and must not be used as production
runtime adapters.

Generated diagnostics and CI artifacts must remain payload-free. Never attach
customer documents, prompts, credentials, or model output to a public report.
