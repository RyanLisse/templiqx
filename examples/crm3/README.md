# CRM3 conformance package

This is a standalone, synthetic Templiqx package used to prove the BLI-61 →
BLI-62 interaction boundary and explicit DOCX V5 compatibility. It imports no
Basenet code and contains no customer or production data.

The conformance test validates and executes BLI-61, accepts its output only
after JSON Schema validation, feeds that value into BLI-62, then migrates and
renders the checked-in V5 template. The approved baseline intentionally retains
one unresolved `${missing.reference}` marker so silent degradation is tested.

BLI-61 evidence is grounded in the exact synthetic source fragment: every fact
records its document and fragment identifiers, UTF-8 byte range, and SHA-256 of
the quoted bytes. BLI-62 receives the schema-valid extraction wholesale; the
notice date is therefore extracted evidence rather than an invented draft fact.

The V5 fixture exercises body paragraphs, a table cell, header and footer story
parts, split-run alias migration, simple and complex Word MERGEFIELDs, repeated
references, and an intentionally unresolved reference. This is the explicitly
selected compatibility subset, not a claim of arbitrary V5/DOCX support.
