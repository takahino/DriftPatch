mod from_commit;
mod report;

use std::path::{Path, PathBuf};

use chrono::Local;

use crate::encoding::{encode_text, read_file_auto};
use crate::lexer::profiles::detect_profile;
use crate::patch::applier::{apply_patch, ApplyError};
use crate::patch::model::PatchFile;
use crate::patch::repository::PatchRepository;

pub use from_commit::{import_from_commit, FromCommitConfig, FromCommitOutcome};
pub use report::{write_html_report, write_xlsx_report, BatchReport, ReportRow, ReportSummary};

#[derive(Debug, Clone)]
pub struct BatchApplyConfig {
    pub work_dir: PathBuf,
    pub patch_dir: PathBuf,
    pub report_dir: PathBuf,
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
    let mut file_cache: std::collections::HashMap<PathBuf, (String, String)> =
        std::collections::HashMap::new();

    for (patch_rel, patch) in patches {
        let row_started = Local::now();
        let target_path = config.work_dir.join(&patch.target_file);

        if patch.target_file.is_empty() {
            rows.push(ReportRow {
                patch_path: patch_rel,
                patch_id: patch.id,
                target_file: String::new(),
                status: "failed".to_string(),
                error_kind: Some("InvalidTarget".to_string()),
                hunk_index: None,
                message: "target_file が空です".to_string(),
                started_at: row_started.format("%Y-%m-%d %H:%M:%S").to_string(),
                finished_at: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            });
            continue;
        }

        if !target_path.exists() {
            rows.push(ReportRow {
                patch_path: patch_rel,
                patch_id: patch.id,
                target_file: patch.target_file.clone(),
                status: "failed".to_string(),
                error_kind: Some("FileNotFound".to_string()),
                hunk_index: None,
                message: format!("対象ファイルが見つかりません: {}", target_path.display()),
                started_at: row_started.format("%Y-%m-%d %H:%M:%S").to_string(),
                finished_at: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            });
            continue;
        }

        let apply_result = apply_single_patch(&target_path, &patch, &mut file_cache);

        let finished_at = Local::now();
        match apply_result {
            Ok(()) => {
                rows.push(ReportRow {
                    patch_path: patch_rel,
                    patch_id: patch.id,
                    target_file: patch.target_file,
                    status: "success".to_string(),
                    error_kind: None,
                    hunk_index: None,
                    message: "適用成功".to_string(),
                    started_at: row_started.format("%Y-%m-%d %H:%M:%S").to_string(),
                    finished_at: finished_at.format("%Y-%m-%d %H:%M:%S").to_string(),
                });
            }
            Err(ApplyFailure::Io(message)) => {
                rows.push(ReportRow {
                    patch_path: patch_rel,
                    patch_id: patch.id,
                    target_file: patch.target_file,
                    status: "failed".to_string(),
                    error_kind: Some("IoError".to_string()),
                    hunk_index: None,
                    message,
                    started_at: row_started.format("%Y-%m-%d %H:%M:%S").to_string(),
                    finished_at: finished_at.format("%Y-%m-%d %H:%M:%S").to_string(),
                });
            }
            Err(ApplyFailure::Patch(e)) => {
                let (error_kind, hunk_index, message) = classify_apply_error(&e);
                rows.push(ReportRow {
                    patch_path: patch_rel,
                    patch_id: patch.id,
                    target_file: patch.target_file,
                    status: "failed".to_string(),
                    error_kind: Some(error_kind),
                    hunk_index,
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

enum ApplyFailure {
    Io(String),
    Patch(ApplyError),
}

fn apply_single_patch(
    target_path: &Path,
    patch: &PatchFile,
    file_cache: &mut std::collections::HashMap<PathBuf, (String, String)>,
) -> Result<(), ApplyFailure> {
    let (text, encoding) = if let Some(cached) = file_cache.get(target_path) {
        cached.clone()
    } else {
        let (text, enc) = read_file_auto(target_path)
            .map_err(|e| ApplyFailure::Io(format!("ファイル読込エラー: {}", e)))?;
        let enc = if patch.encoding.is_empty() {
            enc
        } else {
            patch.encoding.clone()
        };
        file_cache.insert(target_path.to_path_buf(), (text.clone(), enc.clone()));
        (text, enc)
    };

    let profile = detect_profile(target_path);
    let result = apply_patch(&text, patch, profile).map_err(ApplyFailure::Patch)?;

    let enc = if patch.encoding.is_empty() {
        encoding
    } else {
        patch.encoding.clone()
    };
    let bytes = encode_text(&result, &enc);
    std::fs::write(target_path, &bytes)
        .map_err(|e| ApplyFailure::Io(format!("ファイル書込エラー: {}", e)))?;

    file_cache.insert(target_path.to_path_buf(), (result, enc));
    Ok(())
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
}
