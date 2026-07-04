mod from_commit;
mod report;

use std::path::PathBuf;

use chrono::Local;

use crate::patch::applier::ApplyError;
use crate::patch::file_ops::{ApplyOptions, FileOpError, PatchWorkspace};
use crate::patch::repository::PatchRepository;

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

/// FileOpError をレポート用の (エラー種別, ハンク番号, メッセージ) に変換する
fn classify_file_op_error(err: &FileOpError) -> (String, Option<usize>, String) {
    match err {
        FileOpError::Apply(e) => classify_apply_error(e),
        FileOpError::Io(msg) => ("IoError".to_string(), None, msg.clone()),
        FileOpError::TargetNotFound { .. } => {
            ("FileNotFound".to_string(), None, err.to_string())
        }
        FileOpError::FileAlreadyExists(_) => {
            ("FileAlreadyExists".to_string(), None, err.to_string())
        }
        FileOpError::DeleteVerificationFailed { .. } => {
            ("DeleteVerificationFailed".to_string(), None, err.to_string())
        }
        FileOpError::RenameVerificationFailed { .. } => {
            ("RenameVerificationFailed".to_string(), None, err.to_string())
        }
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
        std::fs::write(src.join("OldName.java"), "class OldName {\n    void x() {}\n}\n").unwrap();

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
        std::fs::write(src.join("OldName.java"), "class OldName {\n    void x() {}\n}\n").unwrap();
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
}
