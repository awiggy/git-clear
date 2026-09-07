use std::path::Path;

use git_agent::merge_ai_candidates::{
    CandidateSkipReason, ConflictCandidate, DeterministicRule, MERGE_AI_CONTEXT_LINES,
    MergeFileKind, collect_merge_ai_candidates, deterministic_candidate,
};
use git_agent::merge_tool::three_way_merge;

fn direct_candidate(base: &[&str], local: &[&str], remote: &[&str]) -> ConflictCandidate {
    ConflictCandidate {
        conflict_index: 0,
        file_kind: MergeFileKind::Code,
        base: base.iter().map(ToString::to_string).collect(),
        local: local.iter().map(ToString::to_string).collect(),
        remote: remote.iter().map(ToString::to_string).collect(),
        context_before: Vec::new(),
        context_after: Vec::new(),
    }
}

#[test]
fn identical_sides_rule_matches_directly() {
    let candidate = direct_candidate(&["old();"], &["new();"], &["new();"]);
    let proposal = deterministic_candidate(&candidate, Path::new("src/main.rs")).unwrap();

    assert_eq!(proposal.rule, DeterministicRule::IdenticalSides);
    assert_eq!(proposal.result_lines, vec!["new();".to_string()]);
    assert!(!proposal.needs_review);
}

#[test]
fn formatting_only_conflict_prefers_the_content_changed_side() {
    let base = "fn main() {\n    call_a();\n}\n";
    let local = "fn main() {\n  call_a();\n}\n";
    let remote = "fn main() {\n\tcall_a();\n}\n";
    let document = three_way_merge(base, local, remote);
    assert_eq!(document.conflicts().len(), 1);

    let report = collect_merge_ai_candidates(Path::new("src/main.rs"), base, local, remote, &document);

    assert_eq!(report.file_skip, None);
    assert_eq!(report.candidates.len(), 1);
    assert_eq!(report.deterministic.len(), 1);
    let proposal = &report.deterministic[0];
    assert_eq!(proposal.rule, DeterministicRule::FormattingOnly);
    assert!(!proposal.needs_review);
    // Both sides only reformatted the same content, so the local (current
    // branch) formatting is the deterministic tie-break.
    assert_eq!(proposal.result_lines, vec!["  call_a();".to_string()]);
    assert!(report.unresolved_candidate_indices().is_empty());
}

#[test]
fn formatting_only_requires_review_for_whitespace_significant_files() {
    let base = "def run():\n    call_a()\n";
    let local = "def run():\n  call_a()\n";
    let remote = "def run():\n\tcall_a()\n";
    let document = three_way_merge(base, local, remote);

    let report = collect_merge_ai_candidates(Path::new("app.py"), base, local, remote, &document);

    assert_eq!(report.deterministic.len(), 1);
    assert_eq!(report.deterministic[0].rule, DeterministicRule::FormattingOnly);
    assert!(report.deterministic[0].needs_review);
}

#[test]
fn single_side_removal_applies_when_survivor_only_reformats() {
    let base = "a\nb\nc\n";
    let local = "a\nc\n";
    let remote = "a\n  b  \nc\n";
    let document = three_way_merge(base, local, remote);
    assert_eq!(document.conflicts().len(), 1);

    let report = collect_merge_ai_candidates(Path::new("notes.txt"), base, local, remote, &document);

    assert_eq!(report.deterministic.len(), 1);
    let proposal = &report.deterministic[0];
    assert_eq!(proposal.rule, DeterministicRule::SingleSideRemoval);
    assert!(proposal.result_lines.is_empty());
    assert!(!proposal.needs_review);
}

#[test]
fn delete_versus_modify_has_no_deterministic_candidate() {
    let base = "a\nb\nc\n";
    let local = "a\nc\n";
    let remote = "a\nB\nc\n";
    let document = three_way_merge(base, local, remote);

    let report = collect_merge_ai_candidates(Path::new("notes.txt"), base, local, remote, &document);

    assert_eq!(report.candidates.len(), 1);
    assert!(report.deterministic.is_empty());
    assert_eq!(report.unresolved_candidate_indices(), vec![0]);
}

#[test]
fn whole_file_delete_versus_modify_has_no_deterministic_candidate() {
    let base = "a\nb\n";
    let local = "";
    let remote = "a\nB\n";
    let document = three_way_merge(base, local, remote);
    assert_eq!(document.conflicts().len(), 1);

    let report = collect_merge_ai_candidates(Path::new("notes.txt"), base, local, remote, &document);

    assert!(report.deterministic.is_empty());
    assert_eq!(report.unresolved_candidate_indices(), vec![0]);
}

#[test]
fn non_overlapping_insertions_merge_into_a_union() {
    let candidate = direct_candidate(&["a", "b"], &["a", "X", "b"], &["a", "b", "Y"]);
    let proposal = deterministic_candidate(&candidate, Path::new("src/main.rs")).unwrap();

    assert_eq!(proposal.rule, DeterministicRule::NonOverlappingInsertions);
    assert_eq!(
        proposal.result_lines,
        vec![
            "a".to_string(),
            "X".to_string(),
            "b".to_string(),
            "Y".to_string()
        ]
    );
    assert!(!proposal.needs_review);
}

#[test]
fn overlapping_insertions_stay_unresolved() {
    let candidate = direct_candidate(&["a"], &["a", "X"], &["a", "Y"]);

    assert!(deterministic_candidate(&candidate, Path::new("src/main.rs")).is_none());
}

#[test]
fn insertion_plus_modification_is_not_a_pure_insertion() {
    let candidate = direct_candidate(&["a", "b"], &["a", "X", "b"], &["a", "B"]);

    assert!(deterministic_candidate(&candidate, Path::new("src/main.rs")).is_none());
}

#[test]
fn binary_content_skips_the_whole_file() {
    let base = "a\n";
    let local = "a\0\n";
    let remote = "a\nb\n";
    let document = three_way_merge(base, local, remote);

    let report = collect_merge_ai_candidates(Path::new("data.bin"), base, local, remote, &document);

    assert_eq!(report.file_skip, Some(CandidateSkipReason::BinaryContent));
    assert!(report.candidates.is_empty());
    assert!(report.deterministic.is_empty());
}

#[test]
fn binary_extension_skips_the_whole_file() {
    let base = "a\nb\n";
    let local = "a\nB\n";
    let remote = "a\nc\n";
    let document = three_way_merge(base, local, remote);

    let report = collect_merge_ai_candidates(Path::new("logo.png"), base, local, remote, &document);

    assert_eq!(report.file_skip, Some(CandidateSkipReason::BinaryContent));
    assert!(report.candidates.is_empty());
}

#[test]
fn oversized_files_skip_the_whole_file() {
    let base = "a\n";
    let local = format!("a\n{}", "x\n".repeat(300_000));
    let remote = "a\n";
    let document = three_way_merge(base, "a\n", remote);

    let report =
        collect_merge_ai_candidates(Path::new("big.rs"), base, &local, remote, &document);

    assert_eq!(
        report.file_skip,
        Some(CandidateSkipReason::OversizedFile {
            bytes: local.len(),
            limit: 512 * 1024,
        })
    );
    assert!(report.candidates.is_empty());
}

#[test]
fn sensitive_paths_skip_the_whole_file() {
    let base = "KEY=1\n";
    let local = "KEY=2\n";
    let remote = "KEY=3\n";
    let document = three_way_merge(base, local, remote);

    for path in [".env", ".env.production", "id_rsa", "server.pem", "aws_credentials"] {
        let report =
            collect_merge_ai_candidates(Path::new(path), base, local, remote, &document);
        assert_eq!(
            report.file_skip,
            Some(CandidateSkipReason::SensitivePath),
            "expected {path} to be treated as sensitive"
        );
        assert!(report.candidates.is_empty());
    }
}

#[test]
fn secret_content_skips_the_whole_file() {
    let base = "a\nb\n";
    let local = "a\nB\n-----BEGIN RSA PRIVATE KEY-----\n";
    let remote = "a\nc\n";
    let document = three_way_merge(base, local, remote);

    let report = collect_merge_ai_candidates(Path::new("config.rs"), base, local, remote, &document);

    assert_eq!(report.file_skip, Some(CandidateSkipReason::SecretContent));
    assert!(report.candidates.is_empty());
}

#[test]
fn oversized_conflict_blocks_skip_but_keep_other_conflicts() {
    let huge_base: Vec<String> = (0..250).map(|index| format!("a{index}")).collect();
    let huge_local: Vec<String> = (0..250).map(|index| format!("la{index}")).collect();
    let huge_remote: Vec<String> = (0..250).map(|index| format!("ra{index}")).collect();

    let join = |head: &[String], middle: &str| -> String {
        let mut lines = vec!["start".to_owned()];
        lines.extend(head.iter().cloned());
        // Enough unchanged anchors between the two change regions so the diff
        // keeps them as two independent conflicts.
        lines.extend(["m1", "m2", "m3", "m4"].iter().map(ToString::to_string));
        lines.push(middle.to_owned());
        lines.push("end".to_owned());
        format!("{}\n", lines.join("\n"))
    };
    let base = join(&huge_base, "x");
    let local = join(&huge_local, "x-l");
    let remote = join(&huge_remote, "x-r");
    let document = three_way_merge(&base, &local, &remote);
    assert_eq!(document.conflicts().len(), 2);

    let report =
        collect_merge_ai_candidates(Path::new("src/main.rs"), &base, &local, &remote, &document);

    assert_eq!(report.file_skip, None);
    assert_eq!(report.candidates.len(), 1);
    assert_eq!(report.candidates[0].conflict_index, 1);
    assert_eq!(
        report.skipped_conflicts,
        vec![CandidateSkipReason::OversizedConflict {
            conflict_index: 0,
            lines: 750,
            limit: 600,
        }]
    );
}

#[test]
fn context_lines_are_capped_and_taken_from_the_middle_result() {
    let mut base_lines: Vec<String> = (0..10).map(|index| format!("c{index}")).collect();
    base_lines.push("x".to_owned());
    base_lines.extend((0..10).map(|index| format!("d{index}")));
    let base = format!("{}\n", base_lines.join("\n"));
    let local = base.replace("x\n", "x-l\n");
    let remote = base.replace("x\n", "x-r\n");
    let document = three_way_merge(&base, &local, &remote);
    assert_eq!(document.conflicts().len(), 1);

    let report =
        collect_merge_ai_candidates(Path::new("src/main.rs"), &base, &local, &remote, &document);

    assert_eq!(report.candidates.len(), 1);
    let candidate = &report.candidates[0];
    assert_eq!(candidate.context_before.len(), MERGE_AI_CONTEXT_LINES);
    assert_eq!(candidate.context_after.len(), MERGE_AI_CONTEXT_LINES);
    assert_eq!(candidate.context_before.first().unwrap(), "c4");
    assert_eq!(candidate.context_before.last().unwrap(), "c9");
    assert_eq!(candidate.context_after.first().unwrap(), "d0");
    assert_eq!(candidate.context_after.last().unwrap(), "d5");
    // The conflict line itself must never leak into the context.
    assert!(!candidate.context_before.iter().any(|line| line == "x"));
    assert!(!candidate.context_after.iter().any(|line| line == "x"));
}

#[test]
fn conflict_free_documents_yield_an_empty_report() {
    let base = "a\nb\n";
    let local = "a\nB\n";
    let remote = "a\nb\n";
    let document = three_way_merge(base, local, remote);
    assert!(document.conflicts().is_empty());

    let report =
        collect_merge_ai_candidates(Path::new("src/main.rs"), base, local, remote, &document);

    assert_eq!(report.file_skip, None);
    assert!(report.candidates.is_empty());
    assert!(report.skipped_conflicts.is_empty());
    assert!(report.deterministic.is_empty());
    assert!(report.unresolved_candidate_indices().is_empty());
}
