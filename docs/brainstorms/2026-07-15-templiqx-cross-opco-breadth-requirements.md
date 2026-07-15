---
date: 2026-07-15
topic: templiqx-cross-opco-breadth
---

# Templiqx cross-opco breadth

## Problem Frame

Blinqx-opco’s gebruiken meer dan één soort template: juridische documenten,
e-mail, memo’s, SMS, rapporten, facturen en adviesdocumenten. Templiqx is al
breed als typed, auditable AI-contractlaag, maar de huidige documentadapters
bewijzen nog geen 80/20-dekking voor rijke documentcompositie.

De vraag is daarom niet of Templiqx een volledige HTML/Jinja-engine moet worden.
De vraag is of één veilige contract- en documentlaag de gangbare workflows van
meerdere opco’s kan dragen, met gespecialiseerde renderers als expliciete
escape hatch voor de resterende uitzonderingen.

## Requirements

- **R1. Gedeelde contractlaag:** Templiqx moet dezelfde typed, grounded en
  auditable contractsemantiek bieden voor Legal, Finance/Accountancy,
  Verzekering/Hypotheek, HR en Consultancy/Agency, onafhankelijk van de
  hosttaal of opco.
- **R2. Multi-channel documentoppervlak:** De referentielaag moet ten minste
  veilige HTML/plain-text e-mail, DOCX en PDF kunnen bedienen, en dezelfde
  goedgekeurde merge-data kunnen gebruiken voor memo-, SMS-, rapport- en
  factuurflows. De contractlaag blijft bron van waarheid; outputadapters dragen
  formaat-specifieke escaping en rendering.
- **R3. Legal/Basenet referentiepakket:** Een gesanitiseerd Basenet-pakket moet
  een juridische brief of sommatie kunnen maken met matter/relaties/custom
  fields/financials, bewijsgronding, afdeling-huisstijl, headers/footers,
  tabellen, optionele clausules, bijlagen en ontbrekende-veld-diagnostics.
- **R4. Bounded document composition:** De 80/20-documentvloer omvat
  conditionele regio’s, herhaalde rijen/secties, tabellen, afbeeldingen of
  handtekening-slots, named layouts/branding, voorblad, paginering,
  locale-aware datum/getal/valuta-formattering en reproduceerbare
  DOCX/PDF-render receipts. Dit gebeurt declaratief en fail-closed; geen
  willekeurige templatecode.
- **R5. Drie bewijsbare referentiepakketten:** Naast Legal/Basenet komen er
  minimaal twee pakketten: één gereguleerd adviespakket (Finly/Finteqx-type)
  en één cross-domain workflowpakket (bijvoorbeeld HoorayHR of Simplicate).
  Elk pakket bevat contracten, gesanitiseerde fixtures, ten minste één
  document- of communicatie-uitkomst en evaluaties.
- **R6. Host-owned workflowgrens:** Autorisatie, tenant- en matterdata,
  geautoriseerde query/introspectie, DMS-versies, Word/co-editing, approval,
  taak/send/publish, metering en auditpersisting blijven host-owned. Templiqx
  levert de typed port, policy-relevante diagnostics, fingerprints en
  renderer/adapter-identiteit.
- **R7. Expliciete 20%-escape hatch:** Arbitrary reflection/query-anything,
  vrije Velocity/Jinja/Blade-code, volledige CSS/HTML-fidelity, XLSX-grafieken,
  RTF-compatibiliteit en tax-filing execution mogen via gespecialiseerde
  host-adapters blijven bestaan. Ze mogen niet stilzwijgend als Templiqx-core
  support worden geclaimd.
- **R8. Migratie en preflight:** Basenet-templatecodes, aliases en unsupported
  constructs moeten inspecteerbaar en migreerbaar zijn met preview,
  unresolved-field diagnostics, version/diff/approval en een meetbare
  compatibility report vóór productiegebruik.

## Success Criteria

- Drie referentiepakketten draaien via dezelfde Templiqx-service/API en leveren
  actor-parity voor Rust/CLI/MCP/HTTP waar die surface beschikbaar is.
- Het Legal-pakket maakt vanuit dezelfde grounded input minimaal DOCX, PDF en
  een e-maildraft; ontbrekende of ongeautoriseerde feiten falen gesloten.
- De Legal-fixture bewijst twee of meer partijen, minimaal drie herhaalde
  claim-/regelitems, twee optionele clausules, afdeling-branding en een
  attachment/signature-slot.
- Dezelfde frozen definition is reproduceerbaar op een gecontroleerde
  rendereromgeving; receipts bevatten package/contract/input/evidence/output
  fingerprints plus renderer-identiteit, versie, bytegrootte en hash.
- De drie pakketten tonen samen dat Templiqx de gangbare AI- en documentflows
  dekt zonder te claimen dat het alle legacy-rapportage of HTML-enginegedrag
  vervangt.
- CRM3 grounded-evidence, boundary checks en bestaande conformance blijven
  groen; host-blocked integratiepunten zijn expliciet zichtbaar.

## Scope Boundaries

- Geen volledige Handlebars/Jinja/Velocity-compatibiliteit of arbitrary code
  execution.
- Geen algemene websitegenerator of pixel-perfect CSS/layout-engine.
- Geen XLSX-chart/report-engine in de portable core.
- Geen ownership van Basenet-authz, retrieval/query’s, DMS, WOPI/Word,
  approval, verzending, publicatie, charging of tax execution.
- PDF is een gecontroleerde adapter/outputroute met converter- en
  omgeving-identiteit; PDF/A wordt alleen toegevoegd als een concrete opco-
  use-case dat vereist.
- Een Word-add-in of visual editor is adoptie-/hostwerk, geen voorwaarde voor
  de eerste Templiqx-breedteclaim.

## Key Decisions

- **Twee lagen, één productrichting:** Templiqx is zowel een gedeelde
  AI-contractlaag als een 80/20-documentplatform; de eerste laag is universeel,
  de tweede blijft bounded en adapter-based.
- **Legal bepaalt de zwaarste lat:** Basenet combineert mergevelden,
  afdeling-stationery, juridische structuur, approval en legacy-migratie in één
  referentiepunt.
- **HTML blijft doelgericht:** veilige HTML/plain output voor e-mail en snippets
  is vereist; volledige HTML-engine-pariteit is geen productdoel.
- **Bewijs vóór positionering:** “Blinqx-breed genoeg” vereist drie werkende
  referentiepakketten, niet alleen een capability matrix.
- **Measured compatibility:** elke nieuwe DOCX/PDF-constructie krijgt een
  fixture, expected report en parity-/reproducibility-assertie.

## Dependencies / Assumptions

- Basenet levert een gesanitiseerde juridische fixture en geautoriseerde
  host-data seam; Templiqx krijgt geen productiecredentials of tenantbeleid in
  de portable core.
- De Finly/Finteqx-achtige adviesflow is representatief voor gereguleerde
  documentoutput; de precieze opco-eigenaarschap van sommige GitHub-archetypen
  moet door een domeineigenaar worden bevestigd.
- Qore’s opco-register is richtinggevend maar bevat draft-profielen; claims
  over Finteqx/eBlinqx Fiscaal blijven te verifiëren.

## Outstanding Questions

### Resolve Before Planning

None. De productrichting, bewijsdrempel en scopegrens zijn vastgesteld.

### Deferred to Planning

- **[Affects R4] [Needs research]** Welke eerste bounded DOCX-constructen voor
  herhaalde rijen, conditionele regio’s, images en signatures zijn veilig en
  fixture-proven?
- **[Affects R2/R4] [Technical]** Welke host-owned PDF-converter en welke
  environment/renderer-identiteit zijn nodig voor reproduceerbare output?
- **[Affects R3/R6] [Technical]** Hoe mapt de Basenet-authorized data seam
  matter/relations/custom fields/evidence naar één portable merge-data contract?
- **[Affects R5] [Needs research]** Welk derde pakket (HoorayHR of Simplicate)
  geeft de meeste contractbreedte zonder een tweede rijke documentengine te
  introduceren?

## Evidence Base

- Basenet Linear: BLI-11, BLI-34, BLI-36, BLI-61, BLI-62 en BLI-230.
- Basenet live routes: `/servlets/objects/rela.lettertemplate/searchscreen`,
  `rela.emailtemplate`, `rela.memotemplate`, `rela.smstemplate`,
  `reports.reporttemplate` en `rela.lettertemplate_configuration`.
- Basenet repo: `docs/research/bli-230-report-engine.md`,
  `docs/decisions/proposed/0019-report-generation-engine.md` en
  `packages/domain/src/templates/index.ts`.
- Templiqx: `README.md`, `examples/crm3`,
  `adapters/templiqx-docx-v5/`, `adapters/templiqx-html-plain/` en
  `docs/plans/2026-07-14-001-feat-safe-document-template-capabilities-plan.md`.
- Cross-opco GitHub evidence: `blinqx-hq/tmp-finly-next`,
  `blinqx-hq/salesoptimizer`, `blinqx-hq/azure-cost-reporting` en
  `blinqx-hq/qore-architecture`.

## Next Steps

→ `/prompts:ce-plan` voor een gefaseerd plan rond de drie referentiepakketten,
de Legal DOCX/PDF-floor en de host-owned data/approval seam.
