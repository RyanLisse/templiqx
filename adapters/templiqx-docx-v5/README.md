# Templiqx DOCX V5 adapter

An optional, standalone compatibility adapter for the explicitly enumerated
legacy **V5** subset. It supports `$data.path`, `${path}`, and ordinary Word
`MERGEFIELD` fields in document body, tables, headers, and footers.

The adapter never executes legacy code. BeanShell-like V1 constructs are
reported as `unsafe`; V2 markers, `$func.*`, and unrecognised constructs are
reported as `unsupported`. Migration and rendering are deliberately separate:
the dialect is chosen by the adapter/import request, never guessed during
normal rendering.

Safety limits are applied before OOXML is parsed. Only selected story parts
are changed; every other ZIP member is copied byte-for-byte at the content
level. Output member ordering and timestamps are deterministic.

