mod check;
mod from_commit;
mod report;

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::path::PathBuf;

use chrono::Local;

use crate::patch::applier::ApplyError;
use crate::patch::file_ops::{ApplyOptions, FileOpError, PatchWorkspace};
use crate::patch::model::{PatchFile, PatchKind};
use crate::patch::repository::PatchRepository;

pub use check::{
    check_patches, CheckSeverity, PatchCheckConfig, PatchCheckFinding, PatchCheckOutcome,
};
pub use from_commit::{import_from_commit, FromCommitConfig, FromCommitOutcome};
pub use report::{write_html_report, write_xlsx_report, BatchReport, ReportRow, ReportSummary};

#[derive(Debug, Clone)]
pub struct BatchApplyConfig {
    pub work_dir: PathBuf,
    pub patch_dir: PathBuf,
    pub report_dir: PathBuf,
    /// true ならファイルを一切変更せず、適用可否と予定操作のみレポートする
    pub dry_run: bool,
}

#[derive(Debug, Clone)]
pub struct BatchApplyOutcome {
    pub report: BatchReport,
    pub xlsx_path: PathBuf,
    pub html_path: PathBuf,
}

/// work_dir と patch_dir から全パッチを列挙し、順次適用する。
/// 失敗したパッチは記録して次へ進む。
pub fn apply_all(config: &BatchApplyConfig) -> Result<BatchApplyOutcome, String> {
    let started_at = Local::now();
    let patches = PatchRepository::list_from_dir(&config.patch_dir)
        .map_err(|e| format!("パッチ列挙エラー: {}", e))?;

    // Rename 導入後に辞書順では破綻する依存（Old.java への Modify vs Old→New の Rename など）を
    // 吸収するため、kind / created_at / Rename の依存関係に基づき適用順を整列する。
    let patches = sort_patches_for_apply(patches);

    let mut rows = Vec::new();
    let mut workspace = PatchWorkspace::new(&config.work_dir);
    let opts = ApplyOptions {
        dry_run: config.dry_run,
        create_backup: false,
    };

    for (patch_rel, patch) in patches {
        let row_started = Local::now();

        if patch.target_file.is_empty() {
            rows.push(ReportRow {
                patch_path: patch_rel,
                patch_id: patch.id,
                target_file: String::new(),
                status: "failed".to_string(),
                error_kind: Some("InvalidTarget".to_string()),
                hunk_index: None,
                action: None,
                message: "target_file が空です".to_string(),
                started_at: row_started.format("%Y-%m-%d %H:%M:%S").to_string(),
                finished_at: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            });
            continue;
        }

        let apply_result = workspace.apply(&patch, &opts);

        let finished_at = Local::now();
        match apply_result {
            Ok(action) => {
                rows.push(ReportRow {
                    patch_path: patch_rel,
                    patch_id: patch.id,
                    target_file: patch.target_file,
                    status: "success".to_string(),
                    error_kind: None,
                    hunk_index: None,
                    action: Some(action.kind_str().to_string()),
                    message: action.describe(config.dry_run),
                    started_at: row_started.format("%Y-%m-%d %H:%M:%S").to_string(),
                    finished_at: finished_at.format("%Y-%m-%d %H:%M:%S").to_string(),
                });
            }
            Err(e) => {
                let (error_kind, hunk_index, message) = classify_file_op_error(&e);
                rows.push(ReportRow {
                    patch_path: patch_rel,
                    patch_id: patch.id,
                    target_file: patch.target_file,
                    status: "failed".to_string(),
                    error_kind: Some(error_kind),
                    hunk_index,
                    action: Some(patch.kind.label().to_string()),
                    message,
                    started_at: row_started.format("%Y-%m-%d %H:%M:%S").to_string(),
                    finished_at: finished_at.format("%Y-%m-%d %H:%M:%S").to_string(),
                });
            }
        }
    }

    let finished_at = Local::now();
    let success_count = rows.iter().filter(|r| r.status == "success").count();
    let failed_count = rows.len() - success_count;

    let report = BatchReport {
        work_dir: config.work_dir.display().to_string(),
        patch_dir: config.patch_dir.display().to_string(),
        started_at: started_at.format("%Y-%m-%d %H:%M:%S").to_string(),
        finished_at: finished_at.format("%Y-%m-%d %H:%M:%S").to_string(),
        dry_run: config.dry_run,
        summary: ReportSummary {
            total: rows.len(),
            success: success_count,
            failed: failed_count,
        },
        rows,
    };

    std::fs::create_dir_all(&config.report_dir)
        .map_err(|e| format!("レポートディレクトリ作成エラー: {}", e))?;

    let stamp = started_at.format("%Y%m%d-%H%M%S").to_string();
    let xlsx_path = config
        .report_dir
        .join(format!("driftpatch-report-{}.xlsx", stamp));
    let html_path = config
        .report_dir
        .join(format!("driftpatch-report-{}.html", stamp));

    write_xlsx_report(&report, &xlsx_path)
        .map_err(|e| format!("Excel レポート出力エラー: {}", e))?;
    write_html_report(&report, &html_path)
        .map_err(|e| format!("HTML レポート出力エラー: {}", e))?;

    Ok(BatchApplyOutcome {
        report,
        xlsx_path,
        html_path,
    })
}

/// kind 別の基本適用優先度。Create → Modify → Rename → Delete の順。
fn kind_priority(kind: PatchKind) -> u8 {
    match kind {
        PatchKind::Create => 0,
        PatchKind::Modify => 1,
        PatchKind::Rename => 2,
        PatchKind::Delete => 3,
    }
}

/// `apply_all` 用の決定的な適用順に整列する。
///
/// 基本優先度は `Create -> Modify -> Rename -> Delete`。同一優先度内では
/// `created_at` 昇順、最後に `patch_rel` で安定化する。
///
/// それに加え、Rename がもたらす旧パス・新パスの依存だけは明示的に扱う:
/// - Rename `Old -> New` に対し、`Old` を target_file とする別パッチは Rename より前
///   （Rename 後は Old が存在しないため）
/// - Rename `Old -> New` に対し、`New` を target_file とする別パッチは Rename より後
///   （New は Rename 後に初めて存在するため）
///
/// これらはトポロジカル順序として扱い、基本優先度を同順位のタイブレーカに使う。
/// 循環が検出された場合は（真正の競合）諦めて基本優先度順に落とし、適用時に失敗させる。
fn sort_patches_for_apply(patches: Vec<(String, PatchFile)>) -> Vec<(String, PatchFile)> {
    let n = patches.len();
    if n <= 1 {
        return patches;
    }

    // タイブレーカキー: (priority, created_at, patch_rel)
    let tiebreaker: Vec<(u8, String, String)> = patches
        .iter()
        .map(|(rel, p)| (kind_priority(p.kind), p.created_at.clone(), rel.clone()))
        .collect();

    // 依存エッジ: out_edges[i] = i の後に来るべきノード群
    let mut out_edges: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut in_degree: Vec<usize> = vec![0; n];

    for i in 0..n {
        if patches[i].1.kind != PatchKind::Rename {
            continue;
        }
        let old = patches[i].1.old_path.as_deref().unwrap_or("");
        let new = patches[i].1.target_file.as_str();

        for j in 0..n {
            if i == j {
                continue;
            }
            let target_j = patches[j].1.target_file.as_str();
            if target_j == old {
                // j (Old 宛) は i (Rename) より前
                out_edges[j].push(i);
                in_degree[i] += 1;
            } else if target_j == new {
                // i (Rename) は j (New 宛) より前
                out_edges[i].push(j);
                in_degree[j] += 1;
            }
        }
    }

    // Kahn 法 + 優先度タイブレーカ（min-heap）
    let mut heap: BinaryHeap<Reverse<(u8, String, String, usize)>> = BinaryHeap::new();
    for i in 0..n {
        if in_degree[i] == 0 {
            heap.push(Reverse((
                tiebreaker[i].0,
                tiebreaker[i].1.clone(),
                tiebreaker[i].2.clone(),
                i,
            )));
        }
    }

    let mut order: Vec<usize> = Vec::with_capacity(n);
    while let Some(Reverse((_, _, _, i))) = heap.pop() {
        order.push(i);
        for &j in &out_edges[i] {
            in_degree[j] -= 1;
            if in_degree[j] == 0 {
                heap.push(Reverse((
                    tiebreaker[j].0,
                    tiebreaker[j].1.clone(),
                    tiebreaker[j].2.clone(),
                    j,
                )));
            }
        }
    }

    if order.len() != n {
        // 循環依存（真正の競合）: 基本優先度順に落として適用時に失敗させる
        order = (0..n).collect();
        order.sort_by(|&a, &b| tiebreaker[a].cmp(&tiebreaker[b]));
    }

    order.into_iter().map(|i| patches[i].clone()).collect()
}

/// FileOpError をレポート用の (エラー種別, ハンク番号, メッセージ) に変換する
fn classify_file_op_error(err: &FileOpError) -> (String, Option<usize>, String) {
    match err {
        FileOpError::Apply(e) => classify_apply_error(e),
        FileOpError::Io(msg) => ("IoError".to_string(), None, msg.clone()),
        FileOpError::TargetNotFound { .. } => ("FileNotFound".to_string(), None, err.to_string()),
        FileOpError::FileAlreadyExists(_) => {
            ("FileAlreadyExists".to_string(), None, err.to_string())
        }
        FileOpError::DeleteVerificationFailed { .. } => (
            "DeleteVerificationFailed".to_string(),
            None,
            err.to_string(),
        ),
        FileOpError::RenameVerificationFailed { .. } => (
            "RenameVerificationFailed".to_string(),
            None,
            err.to_string(),
        ),
        FileOpError::InvalidPatch(_) => ("InvalidPatch".to_string(), None, err.to_string()),
    }
}

fn classify_apply_error(err: &ApplyError) -> (String, Option<usize>, String) {
    match err {
        ApplyError::NoMatch { hunk_index } => (
            "NoMatch".to_string(),
            Some(*hunk_index),
            format!("ハンク {} の適用箇所が見つかりませんでした", hunk_index),
        ),
        ApplyError::CountMismatch {
            hunk_index,
            expected,
            actual,
            positions,
        } => (
            "CountMismatch".to_string(),
            Some(*hunk_index),
            format!(
                "ハンク {} の期待マッチ数 {} と実際のマッチ数 {} が一致しません。位置: {:?}",
                hunk_index, expected, actual, positions
            ),
        ),
        ApplyError::OverlappingMatches { hunk_index } => (
            "OverlappingMatches".to_string(),
            Some(*hunk_index),
            format!(
                "ハンク {} の複数マッチの置換範囲が重なっています",
                hunk_index
            ),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::profiles::JAVA;
    use crate::patch::context::ContextConfig;
    use crate::patch::generator::generate_patch;
    use crate::patch::model::PatchFile;
    use crate::patch::repository::PatchRepository;

    #[test]
    fn test_batch_apply_success_and_report() {
        let tmp = std::env::temp_dir().join(format!("driftpatch_batch_{}", uuid::Uuid::new_v4()));
        let work_dir = tmp.join("work");
        let patch_repo = tmp.join("repo");
        let report_dir = tmp.join("reports");
        let target_dir = work_dir.join("src");
        std::fs::create_dir_all(&target_dir).unwrap();

        let target_file = target_dir.join("Foo.java");
        let orig = "void foo() {\n    return null;\n}\n";
        let edit = "void foo() {\n    Objects.requireNonNull(bar);\n    return null;\n}\n";
        std::fs::write(&target_file, orig).unwrap();

        let config = ContextConfig::default();
        let patch = generate_patch(
            orig,
            edit,
            &JAVA,
            "tester",
            "test",
            "src/Foo.java",
            "UTF-8",
            &config,
        )
        .unwrap();

        let repo = PatchRepository::new(&patch_repo);
        repo.save(&patch, "20260628-test.dpatch").unwrap();

        let outcome = apply_all(&BatchApplyConfig {
            work_dir: work_dir.clone(),
            patch_dir: repo.patches_dir(),
            report_dir: report_dir.clone(),
            dry_run: false,
        })
        .unwrap();

        assert_eq!(outcome.report.summary.total, 1);
        assert_eq!(outcome.report.summary.success, 1);
        assert!(outcome.xlsx_path.exists());
        assert!(outcome.html_path.exists());

        let applied = std::fs::read_to_string(&target_file).unwrap();
        assert!(applied.contains("Objects.requireNonNull"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// 削除・新規作成・リネームを含むコミット → from-commit で生成 →
    /// pre-commit 状態の別 work_dir へ適用、の end-to-end フロー用フィクスチャ
    fn setup_kind_repo() -> (std::path::PathBuf, String) {
        use git2::{IndexAddOption, Repository, Signature};

        let tmp = std::env::temp_dir().join(format!("driftpatch_e2e_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        let repo = Repository::init(&tmp).unwrap();
        let src = tmp.join("src");
        std::fs::create_dir_all(&src).unwrap();

        std::fs::write(src.join("Keep.java"), "class Keep {\n    void a() {}\n}\n").unwrap();
        std::fs::write(src.join("Legacy.java"), "class Legacy {}\n").unwrap();
        std::fs::write(
            src.join("OldName.java"),
            "class OldName {\n    void x() {}\n}\n",
        )
        .unwrap();

        let commit_all = |message: &str| {
            let mut index = repo.index().unwrap();
            index
                .add_all(["*"].iter(), IndexAddOption::DEFAULT, None)
                .unwrap();
            index.update_all(["*"].iter(), None).unwrap();
            index.write().unwrap();
            let tree_id = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();
            let sig = Signature::now("tester", "test@example.com").unwrap();
            let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
            if let Some(parent) = parent {
                repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent])
                    .unwrap()
            } else {
                repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[])
                    .unwrap()
            }
        };

        commit_all("initial");

        // 変更 + 削除 + 新規作成 + リネームを 1 コミットに含める
        std::fs::write(
            src.join("Keep.java"),
            "class Keep {\n    void a() { System.out.println(1); }\n}\n",
        )
        .unwrap();
        std::fs::remove_file(src.join("Legacy.java")).unwrap();
        std::fs::write(src.join("Created.java"), "class Created {}\n").unwrap();
        std::fs::rename(src.join("OldName.java"), src.join("NewName.java")).unwrap();
        let oid = commit_all("mixed changes");

        (tmp, oid.to_string())
    }

    /// pre-commit（initial 時点）の状態を別ディレクトリに再現する
    fn setup_pre_commit_workdir(base: &std::path::Path) -> std::path::PathBuf {
        let work_dir = base.join("apply-work");
        let src = work_dir.join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("Keep.java"), "class Keep {\n    void a() {}\n}\n").unwrap();
        std::fs::write(src.join("Legacy.java"), "class Legacy {}\n").unwrap();
        std::fs::write(
            src.join("OldName.java"),
            "class OldName {\n    void x() {}\n}\n",
        )
        .unwrap();
        work_dir
    }

    #[test]
    fn test_batch_apply_create_delete_rename_end_to_end() {
        let (repo_path, commit_sha) = setup_kind_repo();
        let patch_repo = repo_path.join("patch-repo");

        let outcome = import_from_commit(&FromCommitConfig {
            repo: repo_path.clone(),
            commit: commit_sha,
            work_dir: repo_path.clone(),
            patch_repo: patch_repo.clone(),
            author: "tester".to_string(),
            description: Some("e2e".to_string()),
            report_dir: None,
        })
        .unwrap();
        assert_eq!(outcome.failed, 0);
        assert_eq!(outcome.skipped, 0, "全変更がパッチ化されること");

        let work_dir = setup_pre_commit_workdir(&repo_path);
        let report_dir = repo_path.join("reports");

        let apply_outcome = apply_all(&BatchApplyConfig {
            work_dir: work_dir.clone(),
            patch_dir: patch_repo.join("patches"),
            report_dir,
            dry_run: false,
        })
        .unwrap();

        assert_eq!(
            apply_outcome.report.summary.failed, 0,
            "rows: {:?}",
            apply_outcome.report.rows
        );

        // コミット後の状態が再現されること
        let src = work_dir.join("src");
        assert!(
            std::fs::read_to_string(src.join("Keep.java"))
                .unwrap()
                .contains("System.out.println"),
            "Modify が適用されること"
        );
        assert!(!src.join("Legacy.java").exists(), "Delete が適用されること");
        assert_eq!(
            std::fs::read_to_string(src.join("Created.java")).unwrap(),
            "class Created {}\n",
            "Create が適用されること"
        );
        assert!(!src.join("OldName.java").exists(), "Rename 元が消えること");
        assert!(src.join("NewName.java").exists(), "Rename 先ができること");

        // レポートの操作列に各種別が入ること
        let actions: Vec<_> = apply_outcome
            .report
            .rows
            .iter()
            .filter_map(|r| r.action.clone())
            .collect();
        for expected in ["modify", "create", "delete", "rename"] {
            assert!(
                actions.iter().any(|a| a == expected),
                "操作 {} がレポートに含まれること: {:?}",
                expected,
                actions
            );
        }

        let _ = std::fs::remove_dir_all(&repo_path);
    }

    #[test]
    fn test_batch_apply_dry_run_reports_but_no_changes() {
        let (repo_path, commit_sha) = setup_kind_repo();
        let patch_repo = repo_path.join("patch-repo");

        import_from_commit(&FromCommitConfig {
            repo: repo_path.clone(),
            commit: commit_sha,
            work_dir: repo_path.clone(),
            patch_repo: patch_repo.clone(),
            author: "tester".to_string(),
            description: Some("dry".to_string()),
            report_dir: None,
        })
        .unwrap();

        let work_dir = setup_pre_commit_workdir(&repo_path);
        let report_dir = repo_path.join("reports");

        let outcome = apply_all(&BatchApplyConfig {
            work_dir: work_dir.clone(),
            patch_dir: patch_repo.join("patches"),
            report_dir,
            dry_run: true,
        })
        .unwrap();

        assert!(outcome.report.dry_run);
        assert_eq!(
            outcome.report.summary.failed, 0,
            "rows: {:?}",
            outcome.report.rows
        );
        for row in &outcome.report.rows {
            assert!(
                row.message.starts_with("[dry-run]"),
                "message: {}",
                row.message
            );
        }

        // work_dir が完全に無変更であること
        let src = work_dir.join("src");
        assert_eq!(
            std::fs::read_to_string(src.join("Keep.java")).unwrap(),
            "class Keep {\n    void a() {}\n}\n"
        );
        assert!(src.join("Legacy.java").exists());
        assert!(src.join("OldName.java").exists());
        assert!(!src.join("Created.java").exists());
        assert!(!src.join("NewName.java").exists());

        let _ = std::fs::remove_dir_all(&repo_path);
    }

    #[test]
    fn test_sort_puts_modify_on_old_path_before_rename() {
        // 辞書順では NewName.java/ 配下の Rename が Old.java/ 配下の Modify より
        // 先に来てしまう構成。整列後は Modify(Old) -> Rename(Old->New) になること。
        let orig = "class Old {\n    void a() {}\n}\n";
        let edit = "class Old {\n    void a() { System.out.println(1); }\n}\n";

        let modify = generate_patch(
            orig,
            edit,
            &JAVA,
            "tester",
            "mod old",
            "Old.java",
            "UTF-8",
            &ContextConfig::default(),
        )
        .unwrap();
        let rename = pure_rename_patch_helper("Old.java", "NewName.java", edit);

        // list_from_dir と同じ (rel, patch) 形式で意図的に辞書順逆に並べる
        let input = vec![
            ("NewName.java/ren.dpatch".to_string(), rename),
            ("Old.java/mod.dpatch".to_string(), modify),
        ];

        let sorted = sort_patches_for_apply(input);
        assert_eq!(
            sorted[0].1.kind,
            PatchKind::Modify,
            "Modify(Old) が先になること"
        );
        assert_eq!(sorted[1].1.kind, PatchKind::Rename);
    }

    #[test]
    fn test_sort_puts_modify_on_new_path_after_rename() {
        // Rename(Old->New) + Modify(New): Modify は New が存在しないと失敗するため
        // Rename の後に来る必要がある。基本優先度だけでは Modify が先になってしまうが、
        // 依存エッジで Rename 先に来ること。
        let content = "class X {}\n";
        let rename = pure_rename_patch_helper("Old.java", "New.java", content);
        let modify = generate_patch(
            content,
            "class X { void y() {} }\n",
            &JAVA,
            "tester",
            "mod new",
            "New.java",
            "UTF-8",
            &ContextConfig::default(),
        )
        .unwrap();

        let input = vec![
            ("New.java/mod.dpatch".to_string(), modify),
            ("Old.java/ren.dpatch".to_string(), rename),
        ];

        let sorted = sort_patches_for_apply(input);
        assert_eq!(sorted[0].1.kind, PatchKind::Rename, "Rename が先になること");
        assert_eq!(sorted[1].1.kind, PatchKind::Modify);
    }

    #[test]
    fn test_sort_keeps_created_at_order_for_same_file_modifies() {
        let orig = "void foo() {\n    return null;\n}\nvoid bar() {\n    return 1;\n}\n";
        let step1 = "void foo() {\n    return 0;\n}\nvoid bar() {\n    return 1;\n}\n";
        let step2 = "void foo() {\n    return 0;\n}\nvoid bar() {\n    return 2;\n}\n";

        let mut p1 = generate_patch(
            orig,
            step1,
            &JAVA,
            "tester",
            "s1",
            "Seq.java",
            "UTF-8",
            &ContextConfig::default(),
        )
        .unwrap();
        p1.created_at = "2026-07-04T10:00:00+0900".to_string();
        let mut p2 = generate_patch(
            step1,
            step2,
            &JAVA,
            "tester",
            "s2",
            "Seq.java",
            "UTF-8",
            &ContextConfig::default(),
        )
        .unwrap();
        p2.created_at = "2026-07-04T11:00:00+0900".to_string();

        // ファイル名順では逆に並べておく
        let input = vec![
            ("Seq.java/z-late.dpatch".to_string(), p2.clone()),
            ("Seq.java/a-early.dpatch".to_string(), p1.clone()),
        ];

        let sorted = sort_patches_for_apply(input);
        assert_eq!(sorted[0].1.id, p1.id, "created_at 早い方が先");
        assert_eq!(sorted[1].1.id, p2.id);
    }

    #[test]
    fn test_apply_all_succeeds_with_modify_old_then_rename() {
        // Rename パッチは新パス (NewName.java) 配下に保存されるため辞書順では
        // Rename が先に来て Modify(Old.java) が TargetNotFound になる構成。
        // 整列によって両方成功することを apply_all で確認する。
        let tmp =
            std::env::temp_dir().join(format!("driftpatch_order_e2e_{}", uuid::Uuid::new_v4()));
        let work_dir = tmp.join("work");
        let patch_repo = tmp.join("repo");
        let report_dir = tmp.join("reports");
        let src = work_dir.join("src");
        std::fs::create_dir_all(&src).unwrap();

        let orig = "class Old {\n    void a() {}\n}\n";
        let edited = "class Old {\n    void a() { System.out.println(1); }\n}\n";
        std::fs::write(src.join("Old.java"), orig).unwrap();

        let modify = generate_patch(
            orig,
            edited,
            &JAVA,
            "tester",
            "mod old",
            "src/Old.java",
            "UTF-8",
            &ContextConfig::default(),
        )
        .unwrap();
        let rename = pure_rename_patch_helper("src/Old.java", "src/NewName.java", edited);

        let repo = PatchRepository::new(&patch_repo);
        repo.save(&modify, "mod.dpatch").unwrap();
        repo.save(&rename, "ren.dpatch").unwrap();

        let outcome = apply_all(&BatchApplyConfig {
            work_dir: work_dir.clone(),
            patch_dir: repo.patches_dir(),
            report_dir,
            dry_run: false,
        })
        .unwrap();

        assert_eq!(
            outcome.report.summary.failed, 0,
            "rows: {:?}",
            outcome.report.rows
        );
        assert!(!src.join("Old.java").exists(), "Rename 元が消えること");
        let new_text = std::fs::read_to_string(src.join("NewName.java")).unwrap();
        assert!(
            new_text.contains("System.out.println"),
            "Modify -> Rename の順で編集が反映されていること: {}",
            new_text
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// テスト内で純リネームパッチを組み立てるヘルパ
    fn pure_rename_patch_helper(old: &str, new: &str, content: &str) -> PatchFile {
        use crate::patch::model::PATCH_FORMAT_VERSION;
        use crate::patch::verify::significant_token_texts;
        PatchFile {
            version: PATCH_FORMAT_VERSION.to_string(),
            id: "20260704-rename-test0000".to_string(),
            author: "tester".to_string(),
            created_at: "2026-07-04T09:00:00+0900".to_string(),
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
}
