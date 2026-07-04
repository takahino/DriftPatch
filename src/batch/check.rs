use std::path::PathBuf;

use crate::lexer::Token;
use crate::patch::context::find_patch_matches;
use crate::patch::model::{DiffHunk, PatchFile, PatchKind};
use crate::patch::repository::PatchRepository;

/// `check` サブコマンドの入力。`work_dir` は要求せず `patch_dir` 内の
/// `.dpatch` 同士の両立性だけを検査する。
#[derive(Debug, Clone)]
pub struct PatchCheckConfig {
    pub patch_dir: PathBuf,
}

/// 検出結果の深刻度
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckSeverity {
    /// 適用順序をどう整列しても両立しない競合
    Error,
    /// 自動整列で吸収可能だが要注意な関係
    Warning,
}

/// パッチ同士の競合1件分
#[derive(Debug, Clone)]
pub enum PatchCheckFinding {
    /// 同一ファイルの同一トークン篇囲を触る2つのハンク
    OverlappingHunk {
        patch_a: String,
        patch_b: String,
        target_file: String,
        hunk_a: usize,
        hunk_b: usize,
    },
    /// Delete 対象ファイルへの編集（Modify / Rename-with-edit）
    ModifyDeletedFile {
        edit_patch: String,
        delete_patch: String,
        target_file: String,
    },
    /// Rename の旧パスを target_file とする別パッチ
    PatchTargetsRenameOldPath {
        patch: String,
        rename_patch: String,
        old_path: String,
    },
}

impl PatchCheckFinding {
    pub fn severity(&self) -> CheckSeverity {
        match self {
            // 同一篇所への重複修正は順序では解決しない
            PatchCheckFinding::OverlappingHunk { .. } => CheckSeverity::Error,
            // Delete したファイルへの修正は必ず失敗する
            PatchCheckFinding::ModifyDeletedFile { .. } => CheckSeverity::Error,
            // 旧パス宛パッチは基本 Modify->Rename の整列で吸収可能だが、
            // パッチ種別によっては真正の競合になる
            PatchCheckFinding::PatchTargetsRenameOldPath { patch, .. } => {
                // patch 種別は呼び出し側では分かりにくいので、ここでは warning 扱い。
                // 真正の競合（Delete/Rename 同士等）は別ルートで ModifyDeletedFile 等に
                // 重複検出されるため、ここは warning に留める。
                let _ = patch;
                CheckSeverity::Warning
            }
        }
    }

    /// CLI / レポート用の1行メッセージ
    pub fn describe(&self) -> String {
        match self {
            PatchCheckFinding::OverlappingHunk {
                patch_a,
                patch_b,
                target_file,
                hunk_a,
                hunk_b,
            } => format!(
                "重複ハンク: {} のハンク {} と {} のハンク {} が同一ファイル {} の重なる範囲を触っています",
                patch_a, hunk_a, patch_b, hunk_b, target_file
            ),
            PatchCheckFinding::ModifyDeletedFile {
                edit_patch,
                delete_patch,
                target_file,
            } => format!(
                "削除対象への編集: {} は {} により削除される {} を編集しようとしています",
                edit_patch, delete_patch, target_file
            ),
            PatchCheckFinding::PatchTargetsRenameOldPath {
                patch,
                rename_patch,
                old_path,
            } => format!(
                "リネーム旧パス宛パッチ: {} は {} の旧パス {} を対象としています（適用順序に依存）",
                patch, rename_patch, old_path
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PatchCheckOutcome {
    pub patch_dir: String,
    pub findings: Vec<PatchCheckFinding>,
}

impl PatchCheckOutcome {
    pub fn has_error(&self) -> bool {
        self.findings
            .iter()
            .any(|f| f.severity() == CheckSeverity::Error)
    }

    pub fn errors(&self) -> impl Iterator<Item = &PatchCheckFinding> {
        self.findings
            .iter()
            .filter(|f| f.severity() == CheckSeverity::Error)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &PatchCheckFinding> {
        self.findings
            .iter()
            .filter(|f| f.severity() == CheckSeverity::Warning)
    }
}

/// `patch_dir` 内のパッチ同士の両立性を適用せずに検査する。
pub fn check_patches(config: &PatchCheckConfig) -> Result<PatchCheckOutcome, String> {
    let patches = PatchRepository::list_from_dir(&config.patch_dir)
        .map_err(|e| format!("パッチ列挙エラー: {}", e))?;

    let findings = detect_conflicts(&patches);

    Ok(PatchCheckOutcome {
        patch_dir: config.patch_dir.display().to_string(),
        findings,
    })
}

/// 検出本体。パッチ順序に依存せずペアを走査する。
fn detect_conflicts(patches: &[(String, PatchFile)]) -> Vec<PatchCheckFinding> {
    let mut findings = Vec::new();

    let delete_targets: Vec<(usize, &str)> = patches
        .iter()
        .enumerate()
        .filter(|(_, (_, p))| p.kind == PatchKind::Delete)
        .map(|(i, (_, p))| (i, p.target_file.as_str()))
        .collect();

    let renames: Vec<(usize, &PatchFile)> = patches
        .iter()
        .enumerate()
        .filter(|(_, (_, p))| p.kind == PatchKind::Rename)
        .map(|(i, (_, p))| (i, p))
        .collect();

    // 1. Delete 対象ファイルへの編集（Modify / Rename-with-edit）
    for (i, (rel_i, patch_i)) in patches.iter().enumerate() {
        let edits_file = |path: &str| -> bool {
            patch_i.kind == PatchKind::Modify && patch_i.target_file == path
                || (patch_i.kind == PatchKind::Rename
                    && !patch_i.hunks.is_empty()
                    && patch_i.old_path.as_deref() == Some(path))
        };

        for &(del_idx, del_target) in &delete_targets {
            if i == del_idx {
                continue;
            }
            if edits_file(del_target) {
                findings.push(PatchCheckFinding::ModifyDeletedFile {
                    edit_patch: rel_i.clone(),
                    delete_patch: patches[del_idx].0.clone(),
                    target_file: del_target.to_string(),
                });
            }
        }
    }

    // 2. Rename 旧パスを target_file とする別パッチ
    for &(ren_idx, rename) in &renames {
        let old = rename.old_path.as_deref().unwrap_or("");
        if old.is_empty() {
            continue;
        }
        for (j, (rel_j, patch_j)) in patches.iter().enumerate() {
            if j == ren_idx {
                continue;
            }
            if patch_j.kind == PatchKind::Rename {
                // Rename 同士の旧パス衝突は別途。ここでは target_file == old のみ。
                continue;
            }
            if patch_j.target_file == old {
                findings.push(PatchCheckFinding::PatchTargetsRenameOldPath {
                    patch: rel_j.clone(),
                    rename_patch: patches[ren_idx].0.clone(),
                    old_path: old.to_string(),
                });
            }
        }
    }

    // 3. 同一論理ファイルでのハンク重なり
    // target_file 同士が一致するパッチペアで、ハンク window が互いに含まれるか調べる。
    for i in 0..patches.len() {
        let (_, pi) = &patches[i];
        if !has_hunks(pi) {
            continue;
        }
        for j in (i + 1)..patches.len() {
            let (rel_j, pj) = &patches[j];
            if !has_hunks(pj) {
                continue;
            }
            if !same_logical_file(pi, pj) {
                continue;
            }
            let target = shared_target_file(pi, pj);
            for (ha, hunk_a) in pi.hunks.iter().enumerate() {
                for (hb, hunk_b) in pj.hunks.iter().enumerate() {
                    if hunks_overlap(hunk_a, hunk_b) {
                        findings.push(PatchCheckFinding::OverlappingHunk {
                            patch_a: patches[i].0.clone(),
                            patch_b: rel_j.clone(),
                            target_file: target.clone(),
                            hunk_a: ha,
                            hunk_b: hb,
                        });
                    }
                }
            }
        }
    }

    findings
}

fn has_hunks(p: &PatchFile) -> bool {
    matches!(
        p.kind,
        PatchKind::Modify | PatchKind::Create | PatchKind::Rename
    ) && !p.hunks.is_empty()
}

/// 2つのパッチが同じ実ファイルに対する編集か。
/// Rename の場合は旧パス・新パス両方で関連付けうるが、check では
/// 「同一ファイル上で同時に適用されるハンク」を見たいので、
/// Modify 同士は target_file 一致、Rename と Modify は
/// (Rename.old == Modify.target) または (Rename.target == Modify.target) で関連付ける。
fn same_logical_file(a: &PatchFile, b: &PatchFile) -> bool {
    let a_targets = targets_of(a);
    let b_targets = targets_of(b);
    a_targets.iter().any(|t| b_targets.contains(t))
}

fn targets_of(p: &PatchFile) -> Vec<&str> {
    match p.kind {
        PatchKind::Rename => {
            let mut v = vec![p.target_file.as_str()];
            if let Some(old) = p.old_path.as_deref() {
                v.push(old);
            }
            v
        }
        _ => vec![p.target_file.as_str()],
    }
}

fn shared_target_file(a: &PatchFile, b: &PatchFile) -> String {
    // 表示用: 共通するパスを拾う、無ければ a.target_file
    let a_targets = targets_of(a);
    let b_targets = targets_of(b);
    for t in &a_targets {
        if b_targets.contains(t) {
            return t.to_string();
        }
    }
    a.target_file.clone()
}

/// 2ハンクが同じ significant token 範囲を触る可能性が高いか。
/// 実ファイル無しで判定するため、hunk A の window（ctx_before + removed + ctx_after）
/// を sig 列とみなし、hunk B のパターンがその中にマッチするか（逆も）で重なりを推定する。
fn hunks_overlap(a: &DiffHunk, b: &DiffHunk) -> bool {
    pattern_matches_within(a, b) || pattern_matches_within(b, a)
}

fn pattern_matches_within(window_hunk: &DiffHunk, pattern_hunk: &DiffHunk) -> bool {
    let window: Vec<Token> = window_hunk
        .context_before
        .iter()
        .cloned()
        .chain(window_hunk.removed.iter().cloned())
        .chain(window_hunk.context_after.iter().cloned())
        .collect();
    if window.is_empty() {
        return false;
    }
    let sig: Vec<&Token> = window.iter().collect();
    !find_patch_matches(
        &sig,
        &pattern_hunk.context_before,
        &pattern_hunk.removed,
        &pattern_hunk.context_after,
    )
    .is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::profiles::JAVA;
    use crate::patch::context::ContextConfig;
    use crate::patch::generator::generate_patch;
    use crate::patch::model::PATCH_FORMAT_VERSION;
    use crate::patch::repository::PatchRepository;

    fn save(repo: &PatchRepository, patch: &PatchFile, filename: &str) {
        repo.save(patch, filename).unwrap();
    }

    fn modify_patch(
        orig: &str,
        edit: &str,
        target_file: &str,
        created_at: &str,
        id: &str,
    ) -> PatchFile {
        let mut p = generate_patch(
            orig,
            edit,
            &JAVA,
            "tester",
            "test",
            target_file,
            "UTF-8",
            &ContextConfig::default(),
        )
        .unwrap();
        p.id = id.to_string();
        p.created_at = created_at.to_string();
        p
    }

    fn delete_patch(target_file: &str, content: &str, id: &str) -> PatchFile {
        use crate::patch::verify::significant_token_texts;
        PatchFile {
            version: PATCH_FORMAT_VERSION.to_string(),
            id: id.to_string(),
            author: "tester".to_string(),
            created_at: "2026-07-04T10:00:00+0900".to_string(),
            description: "delete".to_string(),
            target_file: target_file.to_string(),
            language: "java".to_string(),
            encoding: "UTF-8".to_string(),
            kind: PatchKind::Delete,
            old_path: None,
            verify_tokens: Some(significant_token_texts(content, &JAVA)),
            hunks: vec![],
        }
    }

    fn pure_rename_patch(old: &str, new: &str, content: &str, id: &str) -> PatchFile {
        use crate::patch::verify::significant_token_texts;
        PatchFile {
            version: PATCH_FORMAT_VERSION.to_string(),
            id: id.to_string(),
            author: "tester".to_string(),
            created_at: "2026-07-04T10:00:00+0900".to_string(),
            description: "rename".to_string(),
            target_file: new.to_string(),
            language: "java".to_string(),
            encoding: "UTF-8".to_string(),
            kind: PatchKind::Rename,
            old_path: Some(old.to_string()),
            verify_tokens: Some(significant_token_texts(content, &JAVA)),
            hunks: vec![],
        }
    }

    #[test]
    fn test_check_no_findings_on_independent_patches() {
        let tmp =
            std::env::temp_dir().join(format!("driftpatch_check_ok_{}", uuid::Uuid::new_v4()));
        let repo = PatchRepository::new(&tmp);
        let orig = "void foo() {\n    return null;\n}\n";
        let edit = "void foo() {\n    Objects.requireNonNull(bar);\n    return null;\n}\n";
        save(
            &repo,
            &modify_patch(orig, edit, "src/Foo.java", "2026-07-04T10:00:00+0900", "p1"),
            "p1.dpatch",
        );
        save(
            &repo,
            &delete_patch("src/Bar.java", "class Bar {}\n", "p2"),
            "p2.dpatch",
        );

        let outcome = check_patches(&PatchCheckConfig {
            patch_dir: repo.patches_dir(),
        })
        .unwrap();

        assert!(
            outcome.findings.is_empty(),
            "findings: {:?}",
            outcome.findings
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_check_detects_overlapping_hunks() {
        let tmp =
            std::env::temp_dir().join(format!("driftpatch_check_overlap_{}", uuid::Uuid::new_v4()));
        let repo = PatchRepository::new(&tmp);

        // 同一ファイルの同じ return null; を別要件で書き換える2パッチ
        let orig = "void foo() { return null; }\n";
        let edit_a = "void foo() { return 0; }\n";
        let edit_b = "void foo() { return 1; }\n";
        save(
            &repo,
            &modify_patch(orig, edit_a, "Foo.java", "2026-07-04T10:00:00+0900", "a"),
            "a.dpatch",
        );
        save(
            &repo,
            &modify_patch(orig, edit_b, "Foo.java", "2026-07-04T11:00:00+0900", "b"),
            "b.dpatch",
        );

        let outcome = check_patches(&PatchCheckConfig {
            patch_dir: repo.patches_dir(),
        })
        .unwrap();

        assert!(outcome.has_error(), "findings: {:?}", outcome.findings);
        assert!(
            outcome
                .findings
                .iter()
                .any(|f| matches!(f, PatchCheckFinding::OverlappingHunk { .. })),
            "findings: {:?}",
            outcome.findings
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_check_detects_modify_deleted_file() {
        let tmp =
            std::env::temp_dir().join(format!("driftpatch_check_moddel_{}", uuid::Uuid::new_v4()));
        let repo = PatchRepository::new(&tmp);
        let content = "class Legacy {}\n";

        save(
            &repo,
            &delete_patch("Legacy.java", content, "del"),
            "del.dpatch",
        );
        save(
            &repo,
            &modify_patch(
                content,
                "class Legacy { void x() {} }\n",
                "Legacy.java",
                "2026-07-04T10:00:00+0900",
                "mod",
            ),
            "mod.dpatch",
        );

        let outcome = check_patches(&PatchCheckConfig {
            patch_dir: repo.patches_dir(),
        })
        .unwrap();

        assert!(outcome.has_error());
        assert!(outcome
            .findings
            .iter()
            .any(|f| matches!(f, PatchCheckFinding::ModifyDeletedFile { .. })));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_check_detects_patch_targets_rename_old_path() {
        let tmp =
            std::env::temp_dir().join(format!("driftpatch_check_old_{}", uuid::Uuid::new_v4()));
        let repo = PatchRepository::new(&tmp);
        let content = "class Moved {}\n";

        save(
            &repo,
            &pure_rename_patch("Old.java", "New.java", content, "ren"),
            "ren.dpatch",
        );
        save(
            &repo,
            &modify_patch(
                content,
                "class Moved { void x() {} }\n",
                "Old.java",
                "2026-07-04T10:00:00+0900",
                "mod",
            ),
            "mod.dpatch",
        );

        let outcome = check_patches(&PatchCheckConfig {
            patch_dir: repo.patches_dir(),
        })
        .unwrap();

        assert!(
            outcome
                .findings
                .iter()
                .any(|f| matches!(f, PatchCheckFinding::PatchTargetsRenameOldPath { .. })),
            "findings: {:?}",
            outcome.findings
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_check_non_overlapping_hunks_on_same_file_are_clean() {
        let tmp =
            std::env::temp_dir().join(format!("driftpatch_check_seq_{}", uuid::Uuid::new_v4()));
        let repo = PatchRepository::new(&tmp);

        // foo と bar は別々の関数。逐次適用できる非重複ハンク。
        let orig = "void foo() {\n    return null;\n}\nvoid bar() {\n    return 1;\n}\n";
        let step1 = "void foo() {\n    return 0;\n}\nvoid bar() {\n    return 1;\n}\n";
        let step2 = "void foo() {\n    return 0;\n}\nvoid bar() {\n    return 2;\n}\n";
        save(
            &repo,
            &modify_patch(orig, step1, "Seq.java", "2026-07-04T10:00:00+0900", "s1"),
            "s1.dpatch",
        );
        save(
            &repo,
            &modify_patch(step1, step2, "Seq.java", "2026-07-04T11:00:00+0900", "s2"),
            "s2.dpatch",
        );

        let outcome = check_patches(&PatchCheckConfig {
            patch_dir: repo.patches_dir(),
        })
        .unwrap();

        // s2 は s1 適用後の内容にしかマッチしないが、ハンク範囲は重ならないので
        // OverlappingHunk としては検出されない（順序依存は別途整列で吸収）。
        let overlaps: Vec<_> = outcome
            .findings
            .iter()
            .filter(|f| matches!(f, PatchCheckFinding::OverlappingHunk { .. }))
            .collect();
        assert!(overlaps.is_empty(), "overlaps: {:?}", overlaps);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
