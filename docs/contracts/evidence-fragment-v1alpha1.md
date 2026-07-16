---
title: Evidence fragment v1alpha1
---

A portable evidence fragment identifies an exact quoted byte range in source
content. It allows a host to pass grounded evidence from retrieval into a
`templiqx/v1alpha1` contract without exposing retrieval, authorization, or
document-store behavior to the portable core.

## Fragment shape

| Field | Type | Required | Notes |
|-------|------|----------|-------|
| `document_id` | string | yes | Stable, opaque identifier of the source document |
| `fragment_id` | string | yes | Stable, opaque identifier of the source fragment within the document revision |
| `start_offset` | integer | yes | Zero-based UTF-8 byte offset of the first quoted byte; minimum `0` |
| `end_offset` | integer | yes | Exclusive UTF-8 byte offset immediately after the quoted bytes; must be greater than `start_offset` |
| `quote_sha256` | string | yes | Lowercase, 64-character hexadecimal SHA-256 of the exact bytes in `[start_offset, end_offset)` |
| `content_sha256` | string | no | Lowercase, 64-character hexadecimal SHA-256 of the complete UTF-8 byte sequence identified by `fragment_id` |
| `scope` | object | no | Opaque host-supplied retrieval scope metadata, defined below |
| `extensions` | object | no | Host-owned, namespaced extension values; the portable core preserves but does not interpret them |

The required five fields match the evidence values emitted by the BLI-61 CRM3
fixture and passed unchanged in the BLI-62 request. A contract may add an
assertion field such as `field` to associate a fact with a fragment, but that
association is not part of fragment identity.

## Offset and digest invariants

- Offsets are measured over the complete UTF-8 byte sequence identified by
  `fragment_id`, using the half-open range `[start_offset, end_offset)`.
- Both offsets must fall on UTF-8 character boundaries. The range must be
  non-empty and `end_offset` must not exceed the content length.
- `quote_sha256` is always computed from the selected byte range, not from a
  decoded or normalized string. Whitespace, line endings, and Unicode bytes are
  hashed exactly as retrieved.
- When present, `content_sha256` pins the complete byte sequence used to
  interpret the offsets. A mismatch means the offsets and quote must not be
  accepted against that content version.

Consumers must verify the range and `quote_sha256` before accepting the
fragment as evidence. They must fail closed on an invalid range, a non-character
boundary, or a digest mismatch.

## Host scope metadata

`scope` records the authority under which the fragment was retrieved. Its
values are opaque identifiers; the portable core carries them as data and does
not resolve tenants, evaluate permissions, or reproduce authorization policy.
The five-field fragment remains valid for the current CRM3 fixtures. At a real
retrieval-to-execution boundary, the host must supply this scope either on the
fragment or in an authenticated enclosing envelope that is bound to it.

| Field | Type | Required when `scope` is present | Notes |
|-------|------|----------------------------------|-------|
| `host_id` | string | yes | Opaque identity of the system that supplied the fragment |
| `tenant_id` | string | yes | Opaque identity of the host data-isolation scope |
| `authorization_scope_id` | string | yes | Opaque identity of the authorization scope or decision used for retrieval |

The host must bind this metadata to its authorization record outside Templiqx
and must reject reuse under a different scope. No host-specific entity names,
query language, policy rules, credentials, or retrieval parameters belong in
the fragment.

## Document revision checksum extension

A host may bind a fragment to an immutable document revision with a namespaced
extension. The extension key is host-owned, and its value contains
`revision_checksum`, a lowercase 64-character hexadecimal SHA-256 checksum of
that revision. For a BLI-68-style document store, the host maps its revision
checksum into this value and validates it before contract execution.

```json
{
  "extensions": {
    "example.document_revision": {
      "revision_checksum": "17a6b1422ab0623a7c7c0b5f81ed08f5e8dba68cc0225f1f6d22b77613ef58a4"
    }
  }
}
```

`example.document_revision` is illustrative; integrations must use a namespace
they own. The checksum extension is optional because not every source exposes a
revision model. It does not replace `quote_sha256`. `content_sha256` identifies
the exact UTF-8 content used for byte offsets, while the revision checksum may
identify a wider immutable source artifact.

## Example

```json
{
  "document_id": "SYN-DOC-0001",
  "fragment_id": "clause-2",
  "start_offset": 27,
  "end_offset": 43,
  "quote_sha256": "f36295a8293cb1e891373f4e64f211deaff2ac9e15211fda895608eddc4cdcca",
  "content_sha256": "bc508e8559390ac6725e5c1b7e42952319b02d42e606bb325263abb02400a159",
  "scope": {
    "host_id": "SYN-HOST-001",
    "tenant_id": "SYN-TENANT-001",
    "authorization_scope_id": "SYN-AUTH-SCOPE-001"
  },
  "extensions": {
    "example.document_revision": {
      "revision_checksum": "17a6b1422ab0623a7c7c0b5f81ed08f5e8dba68cc0225f1f6d22b77613ef58a4"
    }
  }
}
```

The synthetic CRM3 fixtures currently exercise the required five-field subset.
Optional metadata is additive and must not change the meaning of those fields.

## Boundaries

- The fragment is portable data, not a retrieval request or authorization token.
- Hosts own source lookup, authorization, tenant isolation, revision validation,
  and assembly of fragment data into contract inputs.
- Templiqx contracts may validate and propagate the shape but must not use it to
  query a source system or infer missing evidence.
- Fragment digests establish byte identity; they are not signatures and do not
  prove that retrieval was authorized.

## Related documents

- [Portable contract format v1alpha1](v1alpha1.md)
- [Host integration guide](../guides/host-integration.md)
