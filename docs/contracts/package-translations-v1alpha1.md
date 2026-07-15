---
title: Package translation bundles v1alpha1
---

Static, manifest-listed translation artifacts for deterministic i18n in
contracts.

## Manifest

Packages declare locale identifiers in `translations`:

```yaml
translations:
  - en
  - nl
```

Each locale maps to `translations/<locale>.yaml` — a flat YAML map of
string keys to string values. Unknown manifest fields remain rejected.

## Resolution

During `compile_contract` and `render_contract`, the application loads listed
bundles and injects them into render context as
`context._templiqx_translations` (locale → key → value). Hosts choose
`context.locale` and optional `context.fallback_locale`; tenant policy and
external localization services stay host-owned.

The `translate` filter resolves a string key from the interpolated value:

```yaml
- kind: interpolate
  expression: { kind: literal, value: greeting.title }
  filters: [translate]
```

Locale lookup order: `context.locale`, `context.fallback_locale`, `en`.
Missing bundles or keys fail closed with `TQX_TRANSLATION_MISSING` or
`TQX_TRANSLATION_KEY`.

## Identity

Translation artifacts participate in package identity and
`validate_package` inventory checks. Editing a bundle changes the package
fingerprint. Unlisted translation files are not read.

## Boundaries

- No executable helper code or host callbacks in bundles.
- No dynamic partial lookup or runtime registry.
- Format-specific escaping remains adapter-owned at document render time.
