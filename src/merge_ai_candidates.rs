//! Stage 1 of the AI auto-merge plan (`docs/merge-ai-auto-merge-plan.md`):
//! deterministic conflict candidate collection with read-only preview data.
//!
//! Nothing in this module writes to `MergeDocument` or the worktree. The
//! output is a read-only report: filtered conflict candidates carrying their
//! base/local/remote texts plus limited context, and deterministic
//! resolutions the merge UI can preview before any AI model is wired in.

use std::collections::BTreeMap;
use std::path::Path;

use crate::merge_tool::{MergeDocument, MergeLanguage};

/// Context lines kept around each conflict block for the read-only preview
/// and for later AI requests.
pub const MERGE_AI_CONTEXT_LINES: usize = 6;
/// Files larger than this are skipped as a whole; merging them is dominated
/// by manual review anyway and AI requests would be truncated.
pub const MERGE_AI_MAX_FILE_BYTES: usize = 512 * 1024;
/// A single conflict block larger than this is skipped; other blocks in the
/// same file are still collected.
pub const MERGE_AI_MAX_CONFLICT_LINES: usize = 600;

/// File classification used to decide whether formatting-only rules may
/// auto-resolve without human review.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MergeFileKind {
    Code,
    Markup,
    Data,
    #[default]
    PlainText,
}

impl MergeFileKind {
    /// In Python/YAML/Makefiles indentation and blank lines are semantic, so
    /// whitespace-based resolutions must stay human-reviewed.
    pub fn whitespace_is_significant(self, path: &Path) -> bool {
        let extension = normalized_extension(path);
        matches!(extension.as_str(), "py" | "pyi" | "yaml" | "yml" | "mk")
            || path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    let name = name.to_lowercase();
                    name == "makefile" || name == "gnumakefile"
                })
    }
}

/// Classify by extension; unknown or missing extensions stay plain text.
pub fn merge_file_kind(path: &Path) -> MergeFileKind {
    match normalized_extension(path).as_str() {
        "rs" | "py" | "pyi" | "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" | "java" | "c" | "h"
        | "cc" | "cpp" | "cxx" | "hpp" | "cs" | "go" | "rb" | "php" | "swift" | "kt" | "kts"
        | "scala" | "sh" | "bash" | "zsh" | "fish" | "ps1" | "pl" | "lua" | "r" | "m" | "mm"
        | "sql" | "html" | "css" | "scss" | "less" | "vue" | "svelte" | "dart" | "ex" | "exs"
        | "erl" | "hrl" | "clj" | "hs" | "ml" | "fs" | "vb" | "asm" | "s" => MergeFileKind::Code,
        "json" | "jsonc" | "yaml" | "yml" | "toml" | "xml" | "ini" | "cfg" | "conf" | "csv"
        | "tsv" | "properties" => MergeFileKind::Data,
        "md" | "markdown" | "rst" | "tex" | "adoc" | "org" => MergeFileKind::Markup,
        _ => MergeFileKind::PlainText,
    }
}

/// Why a file or a single conflict block was excluded from candidate
/// collection. The merge itself is never blocked; only AI/deterministic
/// suggestions are withheld.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CandidateSkipReason {
    BinaryContent,
    OversizedFile { bytes: usize, limit: usize },
    OversizedConflict {
        conflict_index: usize,
        lines: usize,
        limit: usize,
    },
    SensitivePath,
    SecretContent,
}

/// Deterministic rules tried before any AI request, in this order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeterministicRule {
    /// Both sides contain exactly the same lines.
    IdenticalSides,
    /// One side removed the block while the other side made no content
    /// change (identical to base apart from formatting).
    SingleSideRemoval,
    /// Both sides only inserted lines at disjoint positions; the union of
    /// both insertions is unambiguous.
    NonOverlappingInsertions,
    /// Sides differ only by whitespace.
    FormattingOnly,
}

/// Read-only snapshot of one unresolved conflict plus its neighborhood.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConflictCandidate {
    pub conflict_index: usize,
    pub file_kind: MergeFileKind,
    pub base: Vec<String>,
    pub local: Vec<String>,
    pub remote: Vec<String>,
    /// Surrounding lines taken from the current middle result, for preview
    /// rendering and later prompt construction. Never edited in place.
    pub context_before: Vec<String>,
    pub context_after: Vec<String>,
}

/// A deterministic resolution proposal. Applying it is always a separate,
/// user-confirmed step in the merge UI.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeterministicCandidate {
    pub conflict_index: usize,
    pub rule: DeterministicRule,
    pub result_lines: Vec<String>,
    /// True when the rule matched but the file kind makes the outcome
    /// risky (for example whitespace-significant languages).
    pub needs_review: bool,
    pub explanation_zh: String,
    pub explanation_en: String,
}

impl DeterministicCandidate {
    pub fn explanation(&self, language: MergeLanguage) -> &str {
        match language {
            MergeLanguage::Chinese => &self.explanation_zh,
            MergeLanguage::English => &self.explanation_en,
        }
    }
}

/// The complete stage-1 output: what was collected, what was filtered out,
/// and which conflicts already have a deterministic proposal.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MergeCandidateReport {
    pub file_kind: MergeFileKind,
    /// Set when the whole file was filtered before collection.
    pub file_skip: Option<CandidateSkipReason>,
    pub candidates: Vec<ConflictCandidate>,
    pub skipped_conflicts: Vec<CandidateSkipReason>,
    pub deterministic: Vec<DeterministicCandidate>,
}

impl MergeCandidateReport {
    /// Conflicts that still need AI or manual resolution.
    pub fn unresolved_candidate_indices(&self) -> Vec<usize> {
        self.candidates
            .iter()
            .map(|candidate| candidate.conflict_index)
            .filter(|index| {
                !self
                    .deterministic
                    .iter()
                    .any(|proposal| proposal.conflict_index == *index)
            })
            .collect()
    }
}

/// Collect conflict candidates from an unresolved merge document. Pure and
/// read-only: the document and the three source texts are never modified.
pub fn collect_merge_ai_candidates(
    path: &Path,
    base: &str,
    local: &str,
    remote: &str,
    document: &MergeDocument,
) -> MergeCandidateReport {
    let file_kind = merge_file_kind(path);
    let mut report = MergeCandidateReport {
        file_kind,
        ..MergeCandidateReport::default()
    };

    if let Some(reason) = file_skip_reason(path, base, local, remote) {
        report.file_skip = Some(reason);
        return report;
    }

    for conflict in document.conflicts() {
        let block_lines = conflict.base.len() + conflict.local.len() + conflict.remote.len();
        if block_lines > MERGE_AI_MAX_CONFLICT_LINES {
            report
                .skipped_conflicts
                .push(CandidateSkipReason::OversizedConflict {
                    conflict_index: conflict.index,
                    lines: block_lines,
                    limit: MERGE_AI_MAX_CONFLICT_LINES,
                });
            continue;
        }

        let candidate = ConflictCandidate {
            conflict_index: conflict.index,
            file_kind,
            base: conflict.base.clone(),
            local: conflict.local.clone(),
            remote: conflict.remote.clone(),
            context_before: conflict_context(document, conflict.index, true),
            context_after: conflict_context(document, conflict.index, false),
        };
        if let Some(proposal) = deterministic_candidate(&candidate, path) {
            report.deterministic.push(proposal);
        }
        report.candidates.push(candidate);
    }

    report
}

/// Try the deterministic rules against a single conflict candidate. Rules
/// run in a fixed order and the first match wins.
pub fn deterministic_candidate(
    candidate: &ConflictCandidate,
    path: &Path,
) -> Option<DeterministicCandidate> {
    rule_identical_sides(candidate)
        .or_else(|| rule_single_side_removal(candidate))
        .or_else(|| rule_non_overlapping_insertions(candidate))
        .or_else(|| rule_formatting_only(candidate, path))
}

fn file_skip_reason(path: &Path, base: &str, local: &str, remote: &str) -> Option<CandidateSkipReason> {
    if is_sensitive_path(path) {
        return Some(CandidateSkipReason::SensitivePath);
    }
    if is_binary_path(path)
        || base.contains('\0')
        || local.contains('\0')
        || remote.contains('\0')
    {
        return Some(CandidateSkipReason::BinaryContent);
    }
    let bytes = base.len().max(local.len()).max(remote.len());
    if bytes > MERGE_AI_MAX_FILE_BYTES {
        return Some(CandidateSkipReason::OversizedFile {
            bytes,
            limit: MERGE_AI_MAX_FILE_BYTES,
        });
    }
    if [base, local, remote].iter().any(|text| contains_secret_marker(text)) {
        return Some(CandidateSkipReason::SecretContent);
    }
    None
}

fn normalized_extension(path: &Path) -> String {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_lowercase())
        .unwrap_or_default()
}

fn is_binary_path(path: &Path) -> bool {
    matches!(
        normalized_extension(path).as_str(),
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "webp"
            | "ico"
            | "icns"
            | "bmp"
            | "pdf"
            | "zip"
            | "gz"
            | "tgz"
            | "bz2"
            | "xz"
            | "tar"
            | "7z"
            | "rar"
            | "dmg"
            | "exe"
            | "dll"
            | "so"
            | "dylib"
            | "o"
            | "a"
            | "class"
            | "jar"
            | "wasm"
            | "pyc"
            | "ttf"
            | "otf"
            | "woff"
            | "woff2"
            | "mp3"
            | "mp4"
            | "mov"
            | "avi"
            | "mkv"
            | "db"
            | "sqlite"
            | "sqlite3"
    )
}

fn is_sensitive_path(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let name = name.to_lowercase();
    if name == ".env"
        || name.starts_with(".env.")
        || name == ".netrc"
        || name == ".npmrc"
        || name == ".pypirc"
        || name == "id_rsa"
        || name.starts_with("id_rsa.")
        || name == "id_dsa"
        || name.starts_with("id_dsa.")
        || name == "id_ecdsa"
        || name.starts_with("id_ecdsa.")
        || name == "id_ed25519"
        || name.starts_with("id_ed25519.")
    {
        return true;
    }
    if matches!(
        name.rsplit('.').next(),
        Some("pem" | "key" | "p12" | "pfx" | "keystore" | "jks" | "kdbx")
    ) {
        return true;
    }
    name.contains("credential") || name.contains("secret")
}

fn contains_secret_marker(text: &str) -> bool {
    text.contains("PRIVATE KEY-----")
        || text.contains("-----BEGIN PGP MESSAGE-----")
        || text.contains("aws_secret_access_key")
}

/// Lines from the middle result immediately before or after the conflict,
/// capped at `MERGE_AI_CONTEXT_LINES`.
fn conflict_context(document: &MergeDocument, conflict_index: usize, before: bool) -> Vec<String> {
    let positions = document
        .lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.conflict_index == Some(conflict_index))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let (Some(&first), Some(&last)) = (positions.first(), positions.last()) else {
        return Vec::new();
    };
    if before {
        let start = first.saturating_sub(MERGE_AI_CONTEXT_LINES);
        document.lines[start..first]
            .iter()
            .map(|line| line.result.clone())
            .collect()
    } else {
        let end = (last + 1 + MERGE_AI_CONTEXT_LINES).min(document.lines.len());
        document.lines[last + 1..end]
            .iter()
            .map(|line| line.result.clone())
            .collect()
    }
}

fn rule_identical_sides(candidate: &ConflictCandidate) -> Option<DeterministicCandidate> {
    if candidate.local.is_empty() || candidate.local != candidate.remote {
        return None;
    }
    Some(DeterministicCandidate {
        conflict_index: candidate.conflict_index,
        rule: DeterministicRule::IdenticalSides,
        result_lines: candidate.local.clone(),
        needs_review: false,
        explanation_zh: "两侧的修改结果完全一致，采用任一一侧即可。".to_owned(),
        explanation_en: "Both sides contain exactly the same change; taking either side is safe."
            .to_owned(),
    })
}

fn rule_single_side_removal(candidate: &ConflictCandidate) -> Option<DeterministicCandidate> {
    if candidate.base.is_empty() {
        return None;
    }
    let local_removed = candidate.local.is_empty();
    let remote_removed = candidate.remote.is_empty();
    if local_removed == remote_removed {
        return None;
    }
    // Deletion is deterministic only when the surviving side made no content
    // change, meaning it equals base apart from formatting. A delete versus
    // a real modification always stays with the human or the AI.
    let survivor = if local_removed {
        &candidate.remote
    } else {
        &candidate.local
    };
    if normalized_lines(survivor) != normalized_lines(&candidate.base) {
        return None;
    }
    Some(DeterministicCandidate {
        conflict_index: candidate.conflict_index,
        rule: DeterministicRule::SingleSideRemoval,
        result_lines: Vec::new(),
        needs_review: false,
        explanation_zh: "一侧删除了该段，另一侧没有实质内容变化，建议按删除处理。".to_owned(),
        explanation_en: "One side removed this block and the other side made no content change, \
                         so the deletion can be kept."
            .to_owned(),
    })
}

fn rule_non_overlapping_insertions(candidate: &ConflictCandidate) -> Option<DeterministicCandidate> {
    let base = &candidate.base;
    if base.is_empty() {
        return None;
    }
    let local_insertions = insertion_segments(base, &candidate.local)?;
    let remote_insertions = insertion_segments(base, &candidate.remote)?;
    if local_insertions.is_empty() && remote_insertions.is_empty() {
        return None;
    }
    if local_insertions
        .keys()
        .any(|gap| remote_insertions.contains_key(gap))
    {
        return None;
    }

    let mut result_lines = Vec::new();
    for gap in 0..=base.len() {
        if let Some(lines) = local_insertions.get(&gap) {
            result_lines.extend(lines.iter().cloned());
        }
        if let Some(lines) = remote_insertions.get(&gap) {
            result_lines.extend(lines.iter().cloned());
        }
        if let Some(line) = base.get(gap) {
            result_lines.push(line.clone());
        }
    }

    Some(DeterministicCandidate {
        conflict_index: candidate.conflict_index,
        rule: DeterministicRule::NonOverlappingInsertions,
        result_lines,
        needs_review: false,
        explanation_zh: "两侧只在不同位置插入了新内容，互不重叠，可以安全合并为两者的并集。"
            .to_owned(),
        explanation_en: "Both sides only inserted lines at disjoint positions, so the union of \
                         both insertions is unambiguous."
            .to_owned(),
    })
}

fn rule_formatting_only(
    candidate: &ConflictCandidate,
    path: &Path,
) -> Option<DeterministicCandidate> {
    if candidate.local == candidate.remote {
        return None;
    }
    if normalized_lines(&candidate.local) != normalized_lines(&candidate.remote) {
        return None;
    }
    // The rule only fires when both sides are content-identical, so neither
    // side carries a content change relative to base. Take the local (current
    // branch) formatting as the deterministic tie-break.
    let needs_review = candidate.file_kind.whitespace_is_significant(path);
    Some(DeterministicCandidate {
        conflict_index: candidate.conflict_index,
        rule: DeterministicRule::FormattingOnly,
        result_lines: candidate.local.clone(),
        needs_review,
        explanation_zh: "两侧内容一致，仅空白或缩进格式不同，建议采用左侧（当前分支）的版本。"
            .to_owned(),
        explanation_en: "Both sides have identical content and differ only in whitespace; the \
                         left (current) side is kept."
            .to_owned(),
    })
}

/// Whitespace-insensitive line comparison: trim each line and collapse
/// internal whitespace runs, then drop blank lines.
fn normalized_lines(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .map(|line| normalize_line(line))
        .filter(|line| !line.is_empty())
        .collect()
}

fn normalize_line(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Split `side` into insertion segments anchored on `base`, requiring `base`
/// to appear in `side` as an in-order subsequence. Returns `None` when the
/// side modified or removed base content. Keys are gaps: `0` means "before
/// the first base line", `base.len()` means "after the last base line".
fn insertion_segments(base: &[String], side: &[String]) -> Option<BTreeMap<usize, Vec<String>>> {
    let mut segments = BTreeMap::<usize, Vec<String>>::new();
    let mut base_cursor = 0;
    let mut pending = Vec::new();
    for line in side {
        if base.get(base_cursor) == Some(line) {
            if !pending.is_empty() {
                segments.entry(base_cursor).or_insert_with(Vec::new).append(&mut pending);
            }
            base_cursor += 1;
        } else {
            pending.push(line.clone());
        }
    }
    if base_cursor != base.len() {
        return None;
    }
    if !pending.is_empty() {
        segments.entry(base_cursor).or_insert_with(Vec::new).append(&mut pending);
    }
    Some(segments)
}
