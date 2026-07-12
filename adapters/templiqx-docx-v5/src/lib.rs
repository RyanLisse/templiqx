//! Safe compatibility slice for the explicitly selected legacy DOCX V5 dialect.

use quick_xml::events::{BytesStart, BytesText, Event};
use quick_xml::{Reader, Writer, XmlVersion};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use templiqx_ports::{
    DocumentRenderRequest, DocumentRenderResult, DocumentRenderer, LegacyImportAdapter,
    LegacyImportRequest, LegacyImportResult, PortError,
};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

pub const DIALECT: &str = "v5";

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_entries: usize,
    pub max_entry_bytes: u64,
    pub max_total_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_entries: 512,
            max_entry_bytes: 16 * 1024 * 1024,
            max_total_bytes: 64 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Migrated,
    Approximated,
    Unsupported,
    Unsafe,
    Unresolved,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Finding {
    pub category: Category,
    pub part: String,
    pub construct: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompatibilityReport {
    pub dialect: String,
    pub findings: Vec<Finding>,
    pub migrated: usize,
    pub approximated: usize,
    pub unsupported: usize,
    pub unsafe_constructs: usize,
    pub unresolved: usize,
}

impl CompatibilityReport {
    fn from_findings(findings: Vec<Finding>) -> Self {
        let count = |c| findings.iter().filter(|f| f.category == c).count();
        Self {
            dialect: DIALECT.into(),
            migrated: count(Category::Migrated),
            approximated: count(Category::Approximated),
            unsupported: count(Category::Unsupported),
            unsafe_constructs: count(Category::Unsafe),
            unresolved: count(Category::Unresolved),
            findings,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RenderReport {
    pub dialect: String,
    pub replacements: usize,
    pub unresolved: Vec<UnresolvedReference>,
    pub artifact_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct UnresolvedReference {
    pub part: String,
    pub reference: String,
    pub construct: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartParity {
    pub part: String,
    pub equal: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParityReport {
    pub equal: bool,
    pub compared_parts: Vec<PartParity>,
}

#[derive(Debug, thiserror::Error)]
enum AdapterError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid DOCX ZIP: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("invalid OOXML: {0}")]
    Xml(#[from] quick_xml::Error),
    #[error("unsafe package: {0}")]
    Limit(String),
    #[error("invalid aliases: {0}")]
    Aliases(String),
}

impl From<AdapterError> for PortError {
    fn from(value: AdapterError) -> Self {
        match value {
            AdapterError::Limit(message) | AdapterError::Aliases(message) => {
                Self::InvalidData(message)
            }
            other => Self::Io(other.to_string()),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DocxV5Adapter {
    limits: Limits,
}

impl DocxV5Adapter {
    #[must_use]
    pub fn new(limits: Limits) -> Self {
        Self { limits }
    }

    /// Inspects supported story parts without modifying or executing their contents.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid aliases, malformed/unsafe ZIP packages, or I/O failures.
    pub fn analyze(&self, path: &Path, aliases: &Value) -> Result<CompatibilityReport, PortError> {
        let aliases = parse_aliases(aliases)?;
        let package = read_package(path, self.limits)?;
        let mut findings = Vec::new();
        for (part, bytes) in story_parts(&package) {
            analyze_xml(&part, bytes, &aliases, &mut findings)?;
        }
        Ok(CompatibilityReport::from_findings(findings))
    }

    /// Compares the structural OOXML after removing the enumerated volatile attributes.
    ///
    /// # Errors
    ///
    /// Returns an error when either package or selected OOXML part is invalid.
    pub fn compare_normalized(&self, left: &Path, right: &Path) -> Result<ParityReport, PortError> {
        let left = read_package(left, self.limits)?;
        let right = read_package(right, self.limits)?;
        let names: BTreeSet<_> = story_parts(&left)
            .keys()
            .chain(story_parts(&right).keys())
            .cloned()
            .collect();
        let mut compared_parts = Vec::new();
        for name in names {
            let l = left.get(&name).map(|v| normalize_xml(v)).transpose()?;
            let r = right.get(&name).map(|v| normalize_xml(v)).transpose()?;
            let equal = l == r;
            compared_parts.push(PartParity {
                part: name,
                equal,
                detail: (!equal).then(|| match (l, r) {
                    (None, Some(_)) => "part missing from left".into(),
                    (Some(_), None) => "part missing from right".into(),
                    _ => "normalized OOXML differs".into(),
                }),
            });
        }
        Ok(ParityReport {
            equal: compared_parts.iter().all(|p| p.equal),
            compared_parts,
        })
    }
}

impl LegacyImportAdapter for DocxV5Adapter {
    fn migrate(&self, request: &LegacyImportRequest) -> Result<LegacyImportResult, PortError> {
        if !request.dialect.eq_ignore_ascii_case(DIALECT) {
            return Err(PortError::Unsupported(format!(
                "expected explicit dialect `{DIALECT}`, got `{}`",
                request.dialect
            )));
        }
        let aliases = parse_aliases(&request.aliases)?;
        let mut package = read_package(&request.source, self.limits)?;
        let mut findings = Vec::new();
        for (part, bytes) in story_parts_mut(&mut package) {
            analyze_xml(&part, bytes, &aliases, &mut findings)?;
            let source = String::from_utf8_lossy(bytes).into_owned();
            let migrated = migrate_aliases(&source, &aliases);
            *bytes = migrate_split_aliases(migrated.as_bytes(), &aliases)?;
        }
        let report = CompatibilityReport::from_findings(findings);
        if report.unsafe_constructs > 0 {
            let report = serde_json::to_string(&report)
                .map_err(|error| PortError::InvalidData(error.to_string()))?;
            return Err(PortError::Unsupported(format!(
                "unsafe legacy constructs prevent migration; compatibility_report={report}"
            )));
        }
        let output = migrated_path(&request.source);
        write_package(&output, &package)?;
        Ok(LegacyImportResult {
            report: serde_json::to_value(report)
                .map_err(|e| PortError::InvalidData(e.to_string()))?,
            canonical_template: Some(output),
        })
    }
}

impl DocumentRenderer for DocxV5Adapter {
    fn render_document(
        &self,
        request: &DocumentRenderRequest,
    ) -> Result<DocumentRenderResult, PortError> {
        let mut package = read_package(&request.template, self.limits)?;
        let mut replacements = 0;
        let mut unresolved = BTreeSet::new();
        for (part, bytes) in story_parts_mut(&mut package) {
            let (rendered, count, missing) = render_xml(bytes, &request.data, &part)?;
            *bytes = rendered;
            replacements += count;
            unresolved.extend(missing);
        }
        write_package(&request.output, &package)?;
        let report = RenderReport {
            dialect: DIALECT.into(),
            replacements,
            unresolved: unresolved.into_iter().collect(),
            artifact_bytes: fs::metadata(&request.output)
                .map_err(|e| PortError::Io(e.to_string()))?
                .len(),
        };
        Ok(DocumentRenderResult {
            artifact: request.output.clone(),
            report: serde_json::to_value(report)
                .map_err(|e| PortError::InvalidData(e.to_string()))?,
        })
    }
}

fn migrated_path(source: &Path) -> PathBuf {
    let stem = source
        .file_stem()
        .and_then(|v| v.to_str())
        .unwrap_or("template");
    source.with_file_name(format!("{stem}.templiqx-v5.docx"))
}

fn parse_aliases(value: &Value) -> Result<BTreeMap<String, String>, PortError> {
    let object = value.as_object().ok_or_else(|| {
        AdapterError::Aliases("aliases must be a JSON object of old -> new strings".into())
    })?;
    object
        .iter()
        .map(|(k, v)| {
            v.as_str()
                .map(|s| (k.clone(), s.to_owned()))
                .ok_or_else(|| {
                    AdapterError::Aliases(format!("alias `{k}` must map to a string")).into()
                })
        })
        .collect()
}

fn is_story_part(name: &str) -> bool {
    let is_xml = Path::new(name)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("xml"));
    name == "word/document.xml"
        || (name.starts_with("word/header") && is_xml)
        || (name.starts_with("word/footer") && is_xml)
}

fn story_parts(package: &BTreeMap<String, Vec<u8>>) -> BTreeMap<String, &Vec<u8>> {
    package
        .iter()
        .filter(|(n, _)| is_story_part(n))
        .map(|(n, b)| (n.clone(), b))
        .collect()
}

fn story_parts_mut(package: &mut BTreeMap<String, Vec<u8>>) -> Vec<(String, &mut Vec<u8>)> {
    package
        .iter_mut()
        .filter(|(n, _)| is_story_part(n))
        .map(|(n, b)| (n.clone(), b))
        .collect()
}

fn read_package(path: &Path, limits: Limits) -> Result<BTreeMap<String, Vec<u8>>, PortError> {
    let file = File::open(path).map_err(|e| PortError::Io(e.to_string()))?;
    let mut archive = ZipArchive::new(file).map_err(AdapterError::from)?;
    if archive.len() > limits.max_entries {
        return Err(AdapterError::Limit(format!(
            "ZIP has {} entries; maximum is {}",
            archive.len(),
            limits.max_entries
        ))
        .into());
    }
    let mut total = 0_u64;
    let mut result = BTreeMap::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(AdapterError::from)?;
        let name = entry
            .enclosed_name()
            .ok_or_else(|| {
                AdapterError::Limit(format!("unsafe ZIP member name `{}`", entry.name()))
            })?
            .clone();
        let name = name.to_string_lossy().replace('\\', "/");
        if entry.is_dir() {
            continue;
        }
        if entry.size() > limits.max_entry_bytes {
            return Err(AdapterError::Limit(format!(
                "ZIP member `{name}` exceeds per-entry limit"
            ))
            .into());
        }
        let mut bytes = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(0));
        entry
            .take(limits.max_entry_bytes + 1)
            .read_to_end(&mut bytes)
            .map_err(AdapterError::from)?;
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > limits.max_entry_bytes {
            return Err(AdapterError::Limit(format!(
                "ZIP member `{name}` expanded beyond declared limit"
            ))
            .into());
        }
        total = total
            .checked_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX))
            .ok_or_else(|| AdapterError::Limit("ZIP size overflow".into()))?;
        if total > limits.max_total_bytes {
            return Err(
                AdapterError::Limit("ZIP exceeds total uncompressed-size limit".into()).into(),
            );
        }
        if result.insert(name.clone(), bytes).is_some() {
            return Err(AdapterError::Limit(format!("duplicate ZIP member `{name}`")).into());
        }
    }
    if !result.contains_key("word/document.xml") {
        return Err(PortError::InvalidData(
            "DOCX lacks word/document.xml".into(),
        ));
    }
    Ok(result)
}

fn write_package(path: &Path, package: &BTreeMap<String, Vec<u8>>) -> Result<(), PortError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|e| PortError::Io(e.to_string()))?;
    if fs::symlink_metadata(parent)
        .map_err(|e| PortError::Io(e.to_string()))?
        .file_type()
        .is_symlink()
    {
        return Err(PortError::InvalidPath(format!(
            "refusing output through symlinked parent `{}`",
            parent.display()
        )));
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(PortError::InvalidPath(format!(
                "refusing to replace output symlink `{}`",
                path.display()
            )));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(PortError::Io(error.to_string())),
    }
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| PortError::Io(error.to_string()))?;
    let mut writer = ZipWriter::new(temporary.as_file_mut());
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::default())
        .unix_permissions(0o644);
    for (name, bytes) in package {
        writer
            .start_file(name, options)
            .map_err(AdapterError::from)?;
        writer.write_all(bytes).map_err(AdapterError::from)?;
    }
    writer.finish().map_err(AdapterError::from)?;
    temporary
        .as_file_mut()
        .sync_all()
        .map_err(|error| PortError::Io(error.to_string()))?;
    temporary
        .persist(path)
        .map_err(|error| PortError::Io(error.error.to_string()))?;
    // `NamedTempFile` creates its backing file at mode 0600 (owner-only) as a
    // security default; `persist` renames it into place without touching that
    // mode. Reset it to the same 0644 the zip entries themselves use, so the
    // output is readable by whoever the destination directory already trusts
    // (e.g. a different UID reading a container's bind-mounted output).
    #[cfg(unix)]
    fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o644))
        .map_err(|error| PortError::Io(error.to_string()))?;
    Ok(())
}

fn analyze_xml(
    part: &str,
    xml: &[u8],
    aliases: &BTreeMap<String, String>,
    findings: &mut Vec<Finding>,
) -> Result<(), PortError> {
    let sources = semantic_sources(xml)?;
    let lowered: Vec<String> = sources.iter().map(|source| source.to_lowercase()).collect();
    if lowered.iter().any(|source| {
        ["beanshell", "bsh.", "<%"]
            .iter()
            .any(|marker| source.contains(marker))
    }) {
        findings.push(Finding {
            category: Category::Unsafe,
            part: part.into(),
            construct: "v1_beanshell".into(),
            reference: None,
            detail: "legacy executable content is never executed".into(),
        });
    }
    if lowered.iter().any(|source| {
        ["$func", "${func"]
            .iter()
            .any(|marker| source.contains(marker))
    }) {
        findings.push(Finding {
            category: Category::Unsupported,
            part: part.into(),
            construct: "v5_function".into(),
            reference: None,
            detail: "$func expressions are outside the POC subset".into(),
        });
    }
    if lowered.iter().any(|source| {
        ["$v2", "${v2", "dialect:v2"]
            .iter()
            .any(|marker| source.contains(marker))
    }) {
        findings.push(Finding {
            category: Category::Unsupported,
            part: part.into(),
            construct: "v2".into(),
            reference: None,
            detail: "V2 is detected but not migrated by this adapter".into(),
        });
    }
    for source in &sources {
        for reference in extract_placeholders(source) {
            let mapped = aliases
                .get(&reference)
                .cloned()
                .unwrap_or_else(|| reference.clone());
            findings.push(Finding {
                category: Category::Migrated,
                part: part.into(),
                construct: "v5_reference".into(),
                reference: Some(mapped),
                detail: if aliases.contains_key(&reference) {
                    format!("alias `{reference}` normalized")
                } else {
                    "supported V5 reference".into()
                },
            });
        }
        for reference in extract_mergefield_names(source) {
            let mapped = aliases
                .get(&reference)
                .cloned()
                .unwrap_or_else(|| reference.clone());
            findings.push(Finding {
                category: Category::Migrated,
                part: part.into(),
                construct: "mergefield".into(),
                reference: Some(mapped),
                detail: "ordinary Word MERGEFIELD".into(),
            });
        }
    }
    Ok(())
}

/// Returns semantic text groups rather than scanning serialized XML. Word is
/// free to split both visible text and field instructions over arbitrary runs.
fn semantic_sources(xml: &[u8]) -> Result<Vec<String>, PortError> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(false);
    let mut sources = Vec::new();
    let mut paragraph = String::new();
    let mut paragraph_depth = 0_u32;
    let mut text_element_depth = 0_u32;
    loop {
        match reader.read_event().map_err(AdapterError::from)? {
            Event::Eof => break,
            Event::Start(start) => match local_name(start.name().as_ref()) {
                b"p" => paragraph_depth += 1,
                b"t" | b"instrText" => text_element_depth += 1,
                b"fldSimple" => {
                    for attribute in start.attributes() {
                        let attribute = attribute.map_err(|error| {
                            PortError::InvalidData(format!("invalid OOXML attribute: {error}"))
                        })?;
                        if local_name(attribute.key.as_ref()) == b"instr" {
                            let value = attribute
                                .decoded_and_normalized_value(
                                    XmlVersion::Implicit1_0,
                                    start.decoder(),
                                )
                                .map_err(AdapterError::from)?;
                            sources.push(value.into_owned());
                        }
                    }
                }
                _ => {}
            },
            Event::Empty(start) if local_name(start.name().as_ref()) == b"fldSimple" => {
                if let Some(value) = instruction_attribute(&start)? {
                    sources.push(value);
                }
            }
            Event::End(end) => match local_name(end.name().as_ref()) {
                b"p" => {
                    paragraph_depth = paragraph_depth.saturating_sub(1);
                    if paragraph_depth == 0 && !paragraph.is_empty() {
                        sources.push(std::mem::take(&mut paragraph));
                    }
                }
                b"t" | b"instrText" => {
                    text_element_depth = text_element_depth.saturating_sub(1);
                }
                _ => {}
            },
            Event::Text(text) if text_element_depth > 0 => {
                let decoded = text.decode().map_err(|error| {
                    PortError::InvalidData(format!("invalid OOXML text encoding: {error}"))
                })?;
                let value = quick_xml::escape::unescape(&decoded).map_err(|error| {
                    PortError::InvalidData(format!("invalid OOXML text entity: {error}"))
                })?;
                paragraph.push_str(&value);
            }
            Event::CData(text) if text_element_depth > 0 => {
                let value = text.decode().map_err(|error| {
                    PortError::InvalidData(format!("invalid OOXML CDATA encoding: {error}"))
                })?;
                paragraph.push_str(&value);
            }
            _ => {}
        }
    }
    if !paragraph.is_empty() {
        sources.push(paragraph);
    }
    Ok(sources)
}

fn migrate_aliases(source: &str, aliases: &BTreeMap<String, String>) -> String {
    let mut out = source.to_owned();
    for (old, new) in aliases {
        out = out.replace(&format!("$data.{old}"), &format!("$data.{new}"));
        out = out.replace(&format!("${{{old}}}"), &format!("${{{new}}}"));
        // Field instructions may contain arbitrary whitespace, so replace the field name itself.
        for prefix in ["MERGEFIELD ", "MERGEFIELD  "] {
            out = out.replace(&format!("{prefix}{old}"), &format!("{prefix}{new}"));
        }
    }
    out
}

fn migrate_split_aliases(
    xml: &[u8],
    aliases: &BTreeMap<String, String>,
) -> Result<Vec<u8>, PortError> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(false);
    let mut events = Vec::new();
    loop {
        match reader.read_event().map_err(AdapterError::from)? {
            Event::Eof => break,
            event => events.push(event.into_owned()),
        }
    }
    for event in &mut events {
        if let Event::Start(start) | Event::Empty(start) = event
            && local_name(start.name().as_ref()) == b"fldSimple"
        {
            rewrite_instruction_attribute(start, aliases)?;
        }
    }
    let mut paragraph_depth = 0;
    let mut in_text = false;
    let mut text_indices = Vec::new();
    let mut in_complex_field = false;
    let mut in_instruction_text = false;
    let mut instruction_indices = Vec::new();
    for idx in 0..events.len() {
        match field_char_type(&events[idx]).as_deref() {
            Some("begin") => {
                in_complex_field = true;
                in_instruction_text = false;
                instruction_indices.clear();
            }
            Some("end") if in_complex_field => {
                migrate_instruction_group(&mut events, &instruction_indices, aliases);
                in_complex_field = false;
                in_instruction_text = false;
                instruction_indices.clear();
            }
            _ => {}
        }
        match &events[idx] {
            Event::Start(start) if local_name(start.name().as_ref()) == b"p" => {
                paragraph_depth += 1;
                if paragraph_depth == 1 {
                    text_indices.clear();
                }
            }
            Event::End(end) if local_name(end.name().as_ref()) == b"p" => {
                if paragraph_depth == 1 {
                    migrate_text_group(&mut events, &text_indices, aliases);
                }
                paragraph_depth -= 1;
            }
            Event::Start(start) if local_name(start.name().as_ref()) == b"t" => in_text = true,
            Event::End(end) if local_name(end.name().as_ref()) == b"t" => in_text = false,
            Event::Start(start) if local_name(start.name().as_ref()) == b"instrText" => {
                in_instruction_text = true;
            }
            Event::End(end) if local_name(end.name().as_ref()) == b"instrText" => {
                in_instruction_text = false;
            }
            Event::Text(_) if paragraph_depth > 0 && in_text => text_indices.push(idx),
            Event::Text(_) if in_complex_field && in_instruction_text => {
                instruction_indices.push(idx);
            }
            _ => {}
        }
    }
    if in_complex_field {
        migrate_instruction_group(&mut events, &instruction_indices, aliases);
    }
    let mut writer = Writer::new(Cursor::new(Vec::new()));
    for event in events {
        writer.write_event(event).map_err(AdapterError::from)?;
    }
    Ok(writer.into_inner().into_inner())
}

fn instruction_attribute(start: &BytesStart<'_>) -> Result<Option<String>, PortError> {
    for attribute in start.attributes() {
        let attribute = attribute
            .map_err(|error| PortError::InvalidData(format!("invalid OOXML attribute: {error}")))?;
        if local_name(attribute.key.as_ref()) == b"instr" {
            return attribute
                .decoded_and_normalized_value(XmlVersion::Implicit1_0, start.decoder())
                .map(|value| Some(value.into_owned()))
                .map_err(AdapterError::from)
                .map_err(PortError::from);
        }
    }
    Ok(None)
}

fn rewrite_instruction_attribute(
    start: &mut BytesStart<'static>,
    aliases: &BTreeMap<String, String>,
) -> Result<(), PortError> {
    let decoder = start.decoder();
    let attributes = start
        .attributes()
        .map(|attribute| {
            let attribute = attribute.map_err(|error| {
                PortError::InvalidData(format!("invalid OOXML attribute: {error}"))
            })?;
            let key = attribute.key.as_ref().to_vec();
            let mut value = attribute
                .decoded_and_normalized_value(XmlVersion::Implicit1_0, decoder)
                .map_err(AdapterError::from)?
                .into_owned();
            if local_name(&key) == b"instr" {
                value = migrate_merge_instruction(&value, aliases);
            }
            Ok((key, value))
        })
        .collect::<Result<Vec<_>, PortError>>()?;
    start.clear_attributes();
    for (key, value) in &attributes {
        let key = std::str::from_utf8(key)
            .map_err(|error| PortError::InvalidData(format!("invalid OOXML name: {error}")))?;
        start.push_attribute((key, value.as_str()));
    }
    Ok(())
}

fn migrate_instruction_group(
    events: &mut [Event<'static>],
    indices: &[usize],
    aliases: &BTreeMap<String, String>,
) {
    if indices.is_empty() {
        return;
    }
    let joined = indices
        .iter()
        .filter_map(|index| text_value(&events[*index]))
        .collect::<String>();
    let migrated = migrate_merge_instruction(&joined, aliases);
    if migrated != joined {
        set_text(&mut events[indices[0]], &migrated);
        for index in &indices[1..] {
            set_text(&mut events[*index], "");
        }
    }
}

fn migrate_merge_instruction(instruction: &str, aliases: &BTreeMap<String, String>) -> String {
    let Some(keyword) = mergefield_position(instruction) else {
        return instruction.to_owned();
    };
    let after_keyword = keyword + "MERGEFIELD".len();
    let whitespace = instruction[after_keyword..]
        .find(|character: char| !character.is_whitespace())
        .unwrap_or(instruction.len() - after_keyword);
    let value_start = after_keyword + whitespace;
    let (reference_start, reference_end) = if instruction[value_start..].starts_with('"') {
        let reference_start = value_start + 1;
        let Some(relative_end) = instruction[reference_start..].find('"') else {
            return instruction.to_owned();
        };
        (reference_start, reference_start + relative_end)
    } else {
        let relative_end = instruction[value_start..]
            .find(|character: char| character.is_whitespace() || character == '\\')
            .unwrap_or(instruction.len() - value_start);
        (value_start, value_start + relative_end)
    };
    let Some(replacement) = aliases.get(&instruction[reference_start..reference_end]) else {
        return instruction.to_owned();
    };
    let mut migrated = instruction.to_owned();
    migrated.replace_range(reference_start..reference_end, replacement);
    migrated
}

fn field_char_type(event: &Event<'_>) -> Option<String> {
    let (Event::Empty(start) | Event::Start(start)) = event else {
        return None;
    };
    if local_name(start.name().as_ref()) != b"fldChar" {
        return None;
    }
    start
        .attributes()
        .flatten()
        .find(|attribute| local_name(attribute.key.as_ref()).ends_with(b"fldCharType"))
        .and_then(|attribute| {
            attribute
                .decoded_and_normalized_value(XmlVersion::Implicit1_0, start.decoder())
                .ok()
        })
        .map(std::borrow::Cow::into_owned)
}

fn migrate_text_group(
    events: &mut [Event<'static>],
    indices: &[usize],
    aliases: &BTreeMap<String, String>,
) {
    if indices.is_empty() {
        return;
    }
    let joined: String = indices
        .iter()
        .filter_map(|i| text_value(&events[*i]))
        .collect();
    let migrated = migrate_aliases(&joined, aliases);
    if migrated != joined {
        set_text(&mut events[indices[0]], &migrated);
        for idx in &indices[1..] {
            set_text(&mut events[*idx], "");
        }
    }
}

fn extract_placeholders(source: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut rest = source;
    while let Some(pos) = rest.find("$data.") {
        rest = &rest[pos + 6..];
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')))
            .unwrap_or(rest.len());
        if end > 0 {
            result.push(rest[..end].into());
        }
        rest = &rest[end..];
    }
    let mut rest = source;
    while let Some(pos) = rest.find("${") {
        rest = &rest[pos + 2..];
        if let Some(end) = rest.find('}') {
            let value = &rest[..end];
            if !value.starts_with("func.") && !value.starts_with("v2:") && !value.is_empty() {
                result.push(value.into());
            }
            rest = &rest[end + 1..];
        } else {
            break;
        }
    }
    result
}

fn extract_mergefield_names(source: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = source;
    while let Some(pos) = rest.find("MERGEFIELD") {
        rest = &rest[pos + "MERGEFIELD".len()..];
        let trimmed = rest.trim_start();
        let name = if let Some(stripped) = trimmed.strip_prefix('"') {
            stripped.split('"').next().unwrap_or("")
        } else if let Some(stripped) = trimmed.strip_prefix("&quot;") {
            stripped.split("&quot;").next().unwrap_or("")
        } else {
            trimmed
                .split(|c: char| c.is_whitespace() || matches!(c, '\\' | '<' | '&'))
                .next()
                .unwrap_or("")
        };
        if !name.is_empty() {
            names.push(name.into());
        }
        rest = &trimmed[name.len()..];
    }
    names
}

fn resolve<'a>(data: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.')
        .try_fold(data, |value, segment| value.as_object()?.get(segment))
}

fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(v) => Some(v.clone()),
        Value::Number(v) => Some(v.to_string()),
        Value::Bool(v) => Some(v.to_string()),
        Value::Null => Some(String::new()),
        _ => None,
    }
}

fn replace_references(
    text: &str,
    data: &Value,
    part: &str,
    construct: &str,
    unresolved: &mut BTreeSet<UnresolvedReference>,
) -> (String, usize) {
    let mut output = String::with_capacity(text.len());
    let mut count = 0;
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let tail: String = chars[i..].iter().collect();
        let (prefix, close) = if tail.starts_with("$data.") {
            (6, None)
        } else if tail.starts_with("${") {
            (2, Some('}'))
        } else {
            output.push(chars[i]);
            i += 1;
            continue;
        };
        let start = i + prefix;
        let end = if let Some(close) = close {
            chars[start..]
                .iter()
                .position(|c| *c == close)
                .map(|p| start + p)
        } else {
            Some(
                start
                    + chars[start..]
                        .iter()
                        .position(|c| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')))
                        .unwrap_or(chars.len() - start),
            )
        };
        let Some(end) = end else {
            output.push(chars[i]);
            i += 1;
            continue;
        };
        let reference: String = chars[start..end].iter().collect();
        if reference.starts_with("func.") || reference.starts_with("v2:") || reference.is_empty() {
            output.push(chars[i]);
            i += 1;
            continue;
        }
        if let Some(value) = resolve(data, &reference).and_then(scalar) {
            output.push_str(&value);
            count += 1;
        } else {
            unresolved.insert(UnresolvedReference {
                part: part.into(),
                reference: reference.clone(),
                construct: construct.into(),
            });
            output.extend(chars[i..end + usize::from(close.is_some())].iter());
        }
        i = end + usize::from(close.is_some());
    }
    (output, count)
}

fn render_xml(
    xml: &[u8],
    data: &Value,
    part: &str,
) -> Result<(Vec<u8>, usize, BTreeSet<UnresolvedReference>), PortError> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(false);
    let mut events = Vec::new();
    loop {
        match reader.read_event().map_err(AdapterError::from)? {
            Event::Eof => break,
            event => events.push(event.into_owned()),
        }
    }
    let mut unresolved = BTreeSet::new();
    let mut replacements = 0;
    render_fields(&mut events, data, part, &mut unresolved, &mut replacements);
    render_paragraph_placeholders(&mut events, data, part, &mut unresolved, &mut replacements);
    let mut writer = Writer::new(Cursor::new(Vec::new()));
    for event in events {
        writer.write_event(event).map_err(AdapterError::from)?;
    }
    Ok((writer.into_inner().into_inner(), replacements, unresolved))
}

fn local_name(name: &[u8]) -> &[u8] {
    name.rsplit(|b| *b == b':').next().unwrap_or(name)
}

fn text_value(event: &Event<'_>) -> Option<String> {
    if let Event::Text(text) = event {
        Some(text.decode().ok()?.into_owned())
    } else {
        None
    }
}

fn set_text(event: &mut Event<'static>, value: &str) {
    *event = Event::Text(BytesText::new(value).into_owned());
}

fn render_fields(
    events: &mut [Event<'static>],
    data: &Value,
    part: &str,
    unresolved: &mut BTreeSet<UnresolvedReference>,
    replacements: &mut usize,
) {
    render_simple_fields(events, data, part, unresolved, replacements);
    render_complex_fields(events, data, part, unresolved, replacements);
}

fn render_simple_fields(
    events: &mut [Event<'static>],
    data: &Value,
    part: &str,
    unresolved: &mut BTreeSet<UnresolvedReference>,
    replacements: &mut usize,
) {
    let mut i = 0;
    while i < events.len() {
        let instruction = if let Event::Start(start) = &events[i] {
            if local_name(start.name().as_ref()) == b"fldSimple" {
                start
                    .attributes()
                    .flatten()
                    .find(|a| local_name(a.key.as_ref()) == b"instr")
                    .and_then(|a| {
                        a.decoded_and_normalized_value(XmlVersion::Implicit1_0, start.decoder())
                            .ok()
                    })
                    .map(std::borrow::Cow::into_owned)
            } else {
                None
            }
        } else {
            None
        };
        if let Some(instruction) = instruction
            && let Some(reference) = parse_merge_instruction(&instruction)
        {
            let mut depth = 1;
            let mut j = i + 1;
            let mut target = None;
            while j < events.len() && depth > 0 {
                match &events[j] {
                    Event::Start(_) => depth += 1,
                    Event::End(_) => depth -= 1,
                    Event::Text(_) if target.is_none() => target = Some(j),
                    _ => {}
                }
                j += 1;
            }
            apply_field(
                events,
                target,
                &reference,
                data,
                part,
                unresolved,
                replacements,
            );
        }
        i += 1;
    }
}

fn render_complex_fields(
    events: &mut [Event<'static>],
    data: &Value,
    part: &str,
    unresolved: &mut BTreeSet<UnresolvedReference>,
    replacements: &mut usize,
) {
    let mut begin = None;
    let mut instruction = String::new();
    let mut target = None;
    for idx in 0..events.len() {
        let fld_type = if let Event::Empty(start) | Event::Start(start) = &events[idx] {
            if local_name(start.name().as_ref()) == b"fldChar" {
                start
                    .attributes()
                    .flatten()
                    .find(|a| local_name(a.key.as_ref()).ends_with(b"fldCharType"))
                    .and_then(|a| {
                        a.decoded_and_normalized_value(XmlVersion::Implicit1_0, start.decoder())
                            .ok()
                    })
                    .map(std::borrow::Cow::into_owned)
            } else {
                None
            }
        } else {
            None
        };
        match fld_type.as_deref() {
            Some("begin") => {
                begin = Some(idx);
                instruction.clear();
                target = None;
            }
            Some("separate") if begin.is_some() => target = Some(usize::MAX),
            Some("end") if begin.is_some() => {
                if let Some(reference) = parse_merge_instruction(&instruction) {
                    apply_field(
                        events,
                        target.filter(|i| *i != usize::MAX),
                        &reference,
                        data,
                        part,
                        unresolved,
                        replacements,
                    );
                }
                begin = None;
                instruction.clear();
                target = None;
            }
            _ => {
                if begin.is_some() {
                    if idx > 0
                        && matches!(&events[idx - 1], Event::Start(s) if local_name(s.name().as_ref()) == b"instrText")
                        && let Some(text) = text_value(&events[idx])
                    {
                        instruction.push_str(&text);
                    }
                    if target == Some(usize::MAX) && matches!(&events[idx], Event::Text(_)) {
                        target = Some(idx);
                    }
                }
            }
        }
    }
}

fn parse_merge_instruction(instruction: &str) -> Option<String> {
    let pos = mergefield_position(instruction)?;
    let tail = instruction[pos + "MERGEFIELD".len()..].trim_start();
    if let Some(tail) = tail.strip_prefix('"') {
        return tail
            .split('"')
            .next()
            .filter(|v| !v.is_empty())
            .map(str::to_owned);
    }
    tail.split_whitespace()
        .next()
        .map(|v| v.trim_matches('\\').to_owned())
        .filter(|v| !v.is_empty())
}

fn mergefield_position(instruction: &str) -> Option<usize> {
    instruction
        .as_bytes()
        .windows(b"MERGEFIELD".len())
        .position(|window| window.eq_ignore_ascii_case(b"MERGEFIELD"))
}

fn apply_field(
    events: &mut [Event<'static>],
    target: Option<usize>,
    reference: &str,
    data: &Value,
    part: &str,
    unresolved: &mut BTreeSet<UnresolvedReference>,
    replacements: &mut usize,
) {
    if let Some(value) = resolve(data, reference).and_then(scalar) {
        if let Some(target) = target {
            set_text(&mut events[target], &value);
            *replacements += 1;
        }
    } else {
        unresolved.insert(UnresolvedReference {
            part: part.into(),
            reference: reference.into(),
            construct: "mergefield".into(),
        });
    }
}

fn render_paragraph_placeholders(
    events: &mut [Event<'static>],
    data: &Value,
    part: &str,
    unresolved: &mut BTreeSet<UnresolvedReference>,
    replacements: &mut usize,
) {
    let mut paragraph_depth = 0;
    let mut in_text = false;
    let mut text_indices = Vec::new();
    for idx in 0..events.len() {
        match &events[idx] {
            Event::Start(start) if local_name(start.name().as_ref()) == b"p" => {
                paragraph_depth += 1;
                if paragraph_depth == 1 {
                    text_indices.clear();
                }
            }
            Event::End(end) if local_name(end.name().as_ref()) == b"p" => {
                if paragraph_depth == 1 {
                    render_text_group(events, &text_indices, data, part, unresolved, replacements);
                }
                paragraph_depth -= 1;
            }
            Event::Start(start) if local_name(start.name().as_ref()) == b"t" => in_text = true,
            Event::End(end) if local_name(end.name().as_ref()) == b"t" => in_text = false,
            Event::Text(_) if paragraph_depth > 0 && in_text => text_indices.push(idx),
            _ => {}
        }
    }
}

fn render_text_group(
    events: &mut [Event<'static>],
    indices: &[usize],
    data: &Value,
    part: &str,
    unresolved: &mut BTreeSet<UnresolvedReference>,
    replacements: &mut usize,
) {
    if indices.is_empty() {
        return;
    }
    let joined: String = indices
        .iter()
        .filter_map(|i| text_value(&events[*i]))
        .collect();
    let (rendered, count) = replace_references(&joined, data, part, "v5_reference", unresolved);
    if count > 0 {
        set_text(&mut events[indices[0]], &rendered);
        for idx in &indices[1..] {
            set_text(&mut events[*idx], "");
        }
        *replacements += count;
    }
}

fn normalize_xml(xml: &[u8]) -> Result<Vec<u8>, PortError> {
    let mut reader = Reader::from_reader(xml);
    // Whitespace in Word text nodes is document content. In particular,
    // `xml:space="preserve"` makes leading/trailing spaces visibly significant.
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new(Cursor::new(Vec::new()));
    loop {
        let event = reader.read_event().map_err(AdapterError::from)?;
        match event {
            Event::Eof => break,
            Event::Start(mut start) => {
                let decoder = start.decoder();
                let mut attrs: Vec<_> = start
                    .attributes()
                    .map(|attribute| {
                        attribute.map_err(|error| {
                            PortError::InvalidData(format!("invalid OOXML attribute: {error}"))
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .filter(|a| {
                        let name = local_name(a.key.as_ref());
                        !matches!(
                            name,
                            b"rsidR"
                                | b"rsidRDefault"
                                | b"rsidP"
                                | b"rsidRPr"
                                | b"paraId"
                                | b"textId"
                        )
                    })
                    .map(|a| {
                        a.decoded_and_normalized_value(XmlVersion::Implicit1_0, decoder)
                            .map(|v| (a.key.as_ref().to_vec(), v.into_owned()))
                            .map_err(AdapterError::from)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                attrs.sort_by(|left, right| left.0.cmp(&right.0));
                start.clear_attributes();
                for (key, value) in &attrs {
                    start.push_attribute((key.as_slice(), value.as_bytes()));
                }
                writer
                    .write_event(Event::Start(start))
                    .map_err(AdapterError::from)?;
            }
            Event::Empty(mut start) => {
                let decoder = start.decoder();
                let mut attrs: Vec<_> = start
                    .attributes()
                    .map(|attribute| {
                        attribute.map_err(|error| {
                            PortError::InvalidData(format!("invalid OOXML attribute: {error}"))
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .filter(|a| {
                        let name = local_name(a.key.as_ref());
                        !matches!(
                            name,
                            b"rsidR"
                                | b"rsidRDefault"
                                | b"rsidP"
                                | b"rsidRPr"
                                | b"paraId"
                                | b"textId"
                        )
                    })
                    .map(|a| {
                        a.decoded_and_normalized_value(XmlVersion::Implicit1_0, decoder)
                            .map(|v| (a.key.as_ref().to_vec(), v.into_owned()))
                            .map_err(AdapterError::from)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                attrs.sort_by(|left, right| left.0.cmp(&right.0));
                start.clear_attributes();
                for (key, value) in &attrs {
                    start.push_attribute((key.as_slice(), value.as_bytes()));
                }
                writer
                    .write_event(Event::Empty(start))
                    .map_err(AdapterError::from)?;
            }
            other => writer.write_event(other).map_err(AdapterError::from)?,
        }
    }
    Ok(writer.into_inner().into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    fn fixture(path: &Path, document: &str, header: Option<&str>) {
        let mut package = BTreeMap::new();
        package.insert("[Content_Types].xml".into(), b"<Types/>".to_vec());
        package.insert("word/document.xml".into(), document.as_bytes().to_vec());
        package.insert("word/media/untouched.bin".into(), vec![0, 1, 2, 3]);
        if let Some(header) = header {
            package.insert("word/header1.xml".into(), header.as_bytes().to_vec());
        }
        write_package(path, &package).unwrap();
    }

    #[test]
    fn renders_split_runs_alias_migration_mergefields_and_unresolved() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.docx");
        fixture(
            &source,
            r#"<w:document xmlns:w="w"><w:body><w:p><w:r><w:t>$da</w:t></w:r><w:r><w:t>ta.client.old_name</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:fldSimple w:instr=" MERGEFIELD missing \\* MERGEFORMAT "><w:r><w:t>old</w:t></w:r></w:fldSimple></w:p></w:tc></w:tr></w:tbl></w:body></w:document>"#,
            Some(r#"<w:hdr xmlns:w="w"><w:p><w:r><w:t>${title}</w:t></w:r></w:p></w:hdr>"#),
        );
        let adapter = DocxV5Adapter::default();
        let migrated = adapter
            .migrate(&LegacyImportRequest {
                dialect: "v5".into(),
                source: source.clone(),
                aliases: json!({"client.old_name":"client.name"}),
            })
            .unwrap();
        let migrated_path = migrated.canonical_template.unwrap();
        let output = dir.path().join("output.docx");
        let result = adapter
            .render_document(&DocumentRenderRequest {
                template: migrated_path,
                data: json!({"client":{"name":"Ryan"},"title":"Draft"}),
                output: output.clone(),
            })
            .unwrap();
        let report: RenderReport = serde_json::from_value(result.report).unwrap();
        assert_eq!(report.replacements, 2);
        assert_eq!(
            report.unresolved,
            vec![UnresolvedReference {
                part: "word/document.xml".into(),
                reference: "missing".into(),
                construct: "mergefield".into()
            }]
        );
        let package = read_package(&output, Limits::default()).unwrap();
        assert!(String::from_utf8_lossy(&package["word/document.xml"]).contains("Ryan"));
        assert!(String::from_utf8_lossy(&package["word/header1.xml"]).contains("Draft"));
        assert_eq!(package["word/media/untouched.bin"], [0, 1, 2, 3]);
    }

    #[test]
    fn complex_mergefield_renders_and_output_is_deterministic() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.docx");
        fixture(
            &source,
            r#"<w:document xmlns:w="w"><w:body><w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText> MERGEFIELD client.name \\* MERGEFORMAT </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>old</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p></w:body></w:document>"#,
            None,
        );
        let adapter = DocxV5Adapter::default();
        let one = dir.path().join("one.docx");
        let two = dir.path().join("two.docx");
        for output in [&one, &two] {
            adapter
                .render_document(&DocumentRenderRequest {
                    template: source.clone(),
                    data: json!({"client":{"name":"Lisse"}}),
                    output: output.clone(),
                })
                .unwrap();
        }
        assert_eq!(fs::read(one).unwrap(), fs::read(two).unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn rendered_output_is_world_readable() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.docx");
        fixture(
            &source,
            r#"<w:document xmlns:w="w"><w:body><w:p><w:r><w:t>hi</w:t></w:r></w:p></w:body></w:document>"#,
            None,
        );
        let output = dir.path().join("output.docx");
        DocxV5Adapter::default()
            .render_document(&DocumentRenderRequest {
                template: source,
                data: json!({}),
                output: output.clone(),
            })
            .unwrap();
        let mode = fs::metadata(&output).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o644,
            "tempfile::NamedTempFile persists at 0600; write_package must reset it"
        );
    }

    #[test]
    fn reports_unsafe_and_unsupported_without_executing() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.docx");
        fixture(
            &source,
            r#"<w:document xmlns:w="w"><w:body><w:p><w:r><w:t>BeanShell bsh.eval $func.now ${v2:old}</w:t></w:r></w:p></w:body></w:document>"#,
            None,
        );
        let report = DocxV5Adapter::default()
            .analyze(&source, &json!({}))
            .unwrap();
        assert_eq!(report.unsafe_constructs, 1);
        assert_eq!(report.unsupported, 2);
    }

    #[test]
    fn detects_adversarial_markers_split_across_runs_and_instruction_fragments() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.docx");
        fixture(
            &source,
            r#"<w:document xmlns:w="w"><w:body>
                <w:p><w:r><w:t>Bean</w:t></w:r><w:r><w:t>Shell</w:t></w:r></w:p>
                <w:p><w:r><w:t>$fu</w:t></w:r><w:r><w:t>nc.now</w:t></w:r></w:p>
                <w:p><w:r><w:t>${v</w:t></w:r><w:r><w:t>2:old}</w:t></w:r></w:p>
                <w:p><w:r><w:t>&lt;</w:t></w:r><w:r><w:t>% executable</w:t></w:r></w:p>
                <w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r>
                  <w:r><w:instrText> b</w:instrText></w:r>
                  <w:r><w:instrText>sh.eval </w:instrText></w:r>
                  <w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>
            </w:body></w:document>"#,
            None,
        );
        let adapter = DocxV5Adapter::default();
        let report = adapter.analyze(&source, &json!({})).unwrap();
        assert_eq!(report.unsafe_constructs, 1);
        assert_eq!(report.unsupported, 2);

        let error = adapter
            .migrate(&LegacyImportRequest {
                dialect: DIALECT.into(),
                source: source.clone(),
                aliases: json!({}),
            })
            .unwrap_err();
        let PortError::Unsupported(message) = error else {
            panic!("unsafe migration must return a structured unsupported error");
        };
        assert!(message.contains("compatibility_report={"));
        assert!(message.contains("\"unsafe_constructs\":1"));
        assert!(!migrated_path(&source).exists());
    }

    #[test]
    fn empty_field_instruction_unsafe_markers_fail_closed() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("empty-unsafe.docx");
        fixture(
            &source,
            r#"<w:document xmlns:w="w"><w:body><w:p>
                <w:fldSimple w:instr=" BeanShell "/>
                <w:fldSimple w:instr=" bsh.eval() "/>
            </w:p></w:body></w:document>"#,
            None,
        );
        let adapter = DocxV5Adapter::default();
        let report = adapter.analyze(&source, &json!({})).unwrap();
        assert_eq!(report.unsafe_constructs, 1);

        let error = adapter
            .migrate(&LegacyImportRequest {
                dialect: DIALECT.into(),
                source: source.clone(),
                aliases: json!({}),
            })
            .unwrap_err();
        assert!(matches!(error, PortError::Unsupported(_)));
        assert!(!migrated_path(&source).exists());
    }

    #[test]
    fn normalizes_simple_and_split_complex_mergefield_aliases_semantically() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("split-fields.docx");
        fixture(
            &source,
            r#"<w:document xmlns:w="w"><w:body>
                <w:p><w:fldSimple w:instr=" MERGEFIELD   &quot;client.old_name&quot; \* MERGEFORMAT "><w:r><w:t>old simple</w:t></w:r></w:fldSimple></w:p>
                <w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r>
                  <w:r><w:instrText xml:space="preserve"> MERGEFIELD client.old_</w:instrText></w:r>
                  <w:r><w:instrText>name \* MERGEFORMAT </w:instrText></w:r>
                  <w:r><w:fldChar w:fldCharType="separate"/></w:r>
                  <w:r><w:t>old complex</w:t></w:r>
                  <w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>
            </w:body></w:document>"#,
            None,
        );
        let aliases = json!({"client.old_name":"client.name"});
        let adapter = DocxV5Adapter::default();
        let report = adapter.analyze(&source, &aliases).unwrap();
        assert_eq!(
            report
                .findings
                .iter()
                .filter(|finding| {
                    finding.construct == "mergefield"
                        && finding.reference.as_deref() == Some("client.name")
                })
                .count(),
            2
        );

        let migrated = adapter
            .migrate(&LegacyImportRequest {
                dialect: DIALECT.into(),
                source,
                aliases,
            })
            .unwrap()
            .canonical_template
            .unwrap();
        let package = read_package(&migrated, Limits::default()).unwrap();
        let document = &package["word/document.xml"];
        let serialized = String::from_utf8_lossy(document);
        assert!(!serialized.contains("client.old_name"));
        assert!(!serialized.contains("client.old_"));
        assert_eq!(
            semantic_sources(document)
                .unwrap()
                .iter()
                .filter(|source| source.contains("MERGEFIELD") && source.contains("client.name"))
                .count(),
            2
        );

        let output = dir.path().join("rendered.docx");
        let rendered = adapter
            .render_document(&DocumentRenderRequest {
                template: migrated,
                data: json!({"client":{"name":"Ryan"}}),
                output: output.clone(),
            })
            .unwrap();
        let render_report: RenderReport = serde_json::from_value(rendered.report).unwrap();
        assert_eq!(render_report.replacements, 2);
        assert!(render_report.unresolved.is_empty());
        let rendered = read_package(&output, Limits::default()).unwrap();
        assert_eq!(
            String::from_utf8_lossy(&rendered["word/document.xml"])
                .matches("Ryan")
                .count(),
            2
        );
    }

    #[test]
    fn parity_ignores_only_enumerated_volatile_attributes() {
        let dir = TempDir::new().unwrap();
        let left = dir.path().join("left.docx");
        let right = dir.path().join("right.docx");
        fixture(
            &left,
            r#"<w:document xmlns:w="w"><w:body><w:p w:rsidR="1" w14:paraId="aaa" xmlns:w14="w14"><w:r><w:t>Hello</w:t></w:r></w:p></w:body></w:document>"#,
            None,
        );
        fixture(
            &right,
            r#"<w:document xmlns:w="w"><w:body><w:p w:rsidR="2" w14:paraId="bbb" xmlns:w14="w14"><w:r><w:t>Hello</w:t></w:r></w:p></w:body></w:document>"#,
            None,
        );
        assert!(
            DocxV5Adapter::default()
                .compare_normalized(&left, &right)
                .unwrap()
                .equal
        );
    }

    #[test]
    fn parity_never_ignores_visible_preserved_whitespace() {
        let dir = TempDir::new().unwrap();
        let left = dir.path().join("left.docx");
        let right = dir.path().join("right.docx");
        fixture(
            &left,
            r#"<w:document xmlns:w="w"><w:body><w:p><w:r><w:t xml:space="preserve"> legal text </w:t></w:r></w:p></w:body></w:document>"#,
            None,
        );
        fixture(
            &right,
            r#"<w:document xmlns:w="w"><w:body><w:p><w:r><w:t xml:space="preserve">legal text</w:t></w:r></w:p></w:body></w:document>"#,
            None,
        );
        assert!(
            !DocxV5Adapter::default()
                .compare_normalized(&left, &right)
                .unwrap()
                .equal
        );
    }

    #[cfg(unix)]
    #[test]
    fn refuses_output_symlink_without_touching_its_target() {
        use std::os::unix::fs::symlink;

        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.docx");
        fixture(
            &source,
            r#"<w:document xmlns:w="w"><w:body><w:p><w:r><w:t>${name}</w:t></w:r></w:p></w:body></w:document>"#,
            None,
        );
        let target = dir.path().join("target.docx");
        fs::write(&target, b"do not replace").unwrap();
        let output = dir.path().join("output.docx");
        symlink(&target, &output).unwrap();

        let error = DocxV5Adapter::default()
            .render_document(&DocumentRenderRequest {
                template: source,
                data: json!({"name":"Ryan"}),
                output: output.clone(),
            })
            .unwrap_err();
        assert!(matches!(error, PortError::InvalidPath(_)));
        assert_eq!(fs::read(&target).unwrap(), b"do not replace");
        assert!(
            fs::symlink_metadata(output)
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn rejects_wrong_dialect_and_zip_limit() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.docx");
        fixture(&source, "<document>large</document>", None);
        let adapter = DocxV5Adapter::new(Limits {
            max_entries: 1,
            ..Limits::default()
        });
        assert!(adapter.analyze(&source, &json!({})).is_err());
        let adapter = DocxV5Adapter::default();
        assert!(
            adapter
                .migrate(&LegacyImportRequest {
                    dialect: "v2".into(),
                    source,
                    aliases: json!({})
                })
                .is_err()
        );
    }

    #[test]
    fn legacy_corpus_v1_and_v2_expectations_match_reports() {
        let corpus =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/legacy-corpus/fixtures");
        let v1 = corpus.join("v1-beanshell-detected");
        let v2 = corpus.join("v2-marker-detected");
        let v1_expected: Value =
            serde_json::from_slice(&fs::read(v1.join("expected-report.json")).unwrap()).unwrap();
        let v2_expected: Value =
            serde_json::from_slice(&fs::read(v2.join("expected-report.json")).unwrap()).unwrap();

        let dir = TempDir::new().unwrap();
        let v1_source = dir.path().join("v1.docx");
        fixture(
            &v1_source,
            r#"<w:document xmlns:w="w"><w:body><w:p><w:r><w:t>BeanShell bsh.eval</w:t></w:r></w:p></w:body></w:document>"#,
            None,
        );
        let v1_report = DocxV5Adapter::default()
            .analyze(&v1_source, &json!({}))
            .unwrap();
        assert!(
            v1_expected["categories"]
                .as_array()
                .unwrap()
                .iter()
                .any(|category| category == "unsafe")
        );
        assert!(v1_report.unsafe_constructs >= 1);

        let v2_source = dir.path().join("v2.docx");
        fixture(
            &v2_source,
            r#"<w:document xmlns:w="w"><w:body><w:p><w:r><w:t>${v2:legacy-marker}</w:t></w:r></w:p></w:body></w:document>"#,
            None,
        );
        let v2_report = DocxV5Adapter::default()
            .analyze(&v2_source, &json!({}))
            .unwrap();
        assert!(
            v2_expected["categories"]
                .as_array()
                .unwrap()
                .iter()
                .any(|category| category == "unsupported")
        );
        assert!(v2_report.unsupported >= 1);
    }
}
