#set page(paper: "a4", margin: 24mm)
#set text(font: "Liberation Serif", size: 10pt, lang: "nl")
#set heading(numbering: "1.")

= Sommatie openstaande vorderingen

Aan: *{{ client.name }}*

Namens {{ department.name }} sommeren wij u tot betaling van het openstaande
bedrag van *EUR {{ financials.outstanding_total | number:nl-NL }}*.

== Vorderingen

{{#chart claims locale=nl-NL}}

== Behandelaar

Rol: {{ signature_slot.role }}
