# templiqx-html-plain

Optional, host-constructed `DocumentRenderer` that renders a bounded HTML /
plain-text template from merge JSON. Intended for email and web snippets — a
minimal non-DOCX output path (plan 001 U7). It is **not** part of the default
CLI/MCP composition; a host constructs `HtmlPlainAdapter` explicitly, like the
runtime adapters.

## Template syntax (measured scope)

- `{{ field }}` — HTML-escaped scalar lookup in the merge data object.
- `{{#each list}} … {{/each}}` — one level of iteration over a JSON array.
  Inside a block, `{{ this }}` is the current scalar item and `{{ field }}` is a
  field of the current object item.

There is **no** code execution, **no** nested `each`, and **no** conditionals —
these stay out of scope by design (KTD5). This is field interpolation, not a
full template language or the contract content AST. Unknown fields render as an
empty string and are listed in `report.unresolved_fields`. Output is escaped
(`& < > " '`) and deterministic.

## Non-goals

- Not a general HTML templating engine (no Handlebars/Jinja parity).
- Not wired into the default document-render path (DOCX V5 remains the default).
- Not a PDF or ODT renderer.
