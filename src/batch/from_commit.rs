use std::path::PathBuf;

use chrono::Local;

use crate::git_import::generate_patches_from_commit;
use crate::patch::context::ContextConfig;
use crate::patch::repository::PatchRepository;

use super::report::{write_html_report, write_xlsx_report, BatchReport, ReportRow, ReportSummary};

#[derive(Debug, Clone)]
pub struct FromCommitConfig {
    pub repo: PathBuf,
    pub commit: String,
    pub work_dir: PathBuf,
    pub patch_repo: PathBuf,
    pub author: String,
    pub description: Option<String>,
    pub report_dir: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct FromCommitOutcome {
    pub saved: usize,
    pub skipped: usize,
    pub failed: usize,
    pub report: Option<BatchReport>,
    pub xlsx_path: Option<PathBuf>,
    pub html_path: Option<PathBuf>,
}

/// 指定 Git コミットから .dpatch を生成してパッチリポジトリに保存する。
pub fn import_from_commit(config: &FromCommitConfig) -> Result<FromCommitOutcome, String> {
    let started_at = Local::now();
    let ctx_config = ContextConfig::default();

    let result = generate_patches_from_commit(
        &config.repo,
        &config.commit,
        &config.work_dir,
        &config.author,
        config.description.as_deref(),
        &ctx_config,
    )
    .map_err(|e| e.to_string())?;

    let repo = PatchRepository::new(&config.patch_repo);
    let mut rows = Vec::new();
    let mut saved = 0usize;

    for item in &result.generated {
        let row_started = Local::now();
        match repo.save(&item.patch, &item.filename) {
            Ok(path) => {
                saved += 1;
                rows.push(ReportRow {
                    patch_path: path
                        .strip_prefix(repo.patches_dir())
                        .map(|p| p.to_string_lossy().replace('\\', "/"))
                        .unwrap_or_else(|_| item.filename.clone()),
                    patch_id: item.patch.id.clone(),
                    target_file: item.target_file.clone(),
                    status: "success".to_string(),
                    error_kind: None,
                    hunk_index: None,
                    action: Some(item.patch.kind.label().to_string()),
                    message: crate::i18n::tr("fc.saved").to_string(),
                    started_at: row_started.format("%Y-%m-%d %H:%M:%S").to_string(),
                    finished_at: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                });
            }
            Err(e) => {
                rows.push(ReportRow {
                    patch_path: item.filename.clone(),
                    patch_id: item.patch.id.clone(),
                    target_file: item.target_file.clone(),
                    status: "failed".to_string(),
                    error_kind: Some("SaveError".to_string()),
                    hunk_index: None,
                    action: Some(item.patch.kind.label().to_string()),
                    message: e.to_string(),
                    started_at: row_started.format("%Y-%m-%d %H:%M:%S").to_string(),
                    finished_at: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                });
            }
        }
    }

    for skip in &result.skipped {
        let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        rows.push(ReportRow {
            patch_path: String::new(),
            patch_id: String::new(),
            target_file: skip.path.clone(),
            status: "skipped".to_string(),
            error_kind: Some("Skipped".to_string()),
            hunk_index: None,
            action: None,
            message: skip.reason.clone(),
            started_at: now.clone(),
            finished_at: now,
        });
    }

    let finished_at = Local::now();
    let failed = rows.iter().filter(|r| r.status == "failed").count();
    let skipped = result.skipped.len();

    let mut outcome = FromCommitOutcome {
        saved,
        skipped,
        failed,
        report: None,
        xlsx_path: None,
        html_path: None,
    };

    if let Some(ref report_dir) = config.report_dir {
        let success_count = rows.iter().filter(|r| r.status == "success").count();
        let report = BatchReport {
            work_dir: config.work_dir.display().to_string(),
            patch_dir: repo.patches_dir().display().to_string(),
            started_at: started_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            finished_at: finished_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            dry_run: false,
            summary: ReportSummary {
                total: rows.len(),
                success: success_count,
                // from_commit の「スキップ」（バイナリファイル等）は既存の failed 集計に含める。
                // パッチ適用の冪等スキップ（apply_all）とは意味が異なるためここは常に 0。
                skipped: 0,
                failed: failed + skipped,
            },
            rows,
        };

        std::fs::create_dir_all(report_dir).map_err(|e| {
            crate::i18n::tr_args("batch.report_dir_error", &[("err", &e.to_string())])
        })?;

        let stamp = started_at.format("%Y%m%d-%H%M%S").to_string();
        let xlsx_path = report_dir.join(format!("driftpatch-from-commit-{}.xlsx", stamp));
        let html_path = report_dir.join(format!("driftpatch-from-commit-{}.html", stamp));

        write_xlsx_report(&report, &xlsx_path)
            .map_err(|e| crate::i18n::tr_args("batch.xlsx_error", &[("err", &e)]))?;
        write_html_report(&report, &html_path)
            .map_err(|e| crate::i18n::tr_args("batch.html_error", &[("err", &e)]))?;

        outcome.report = Some(report);
        outcome.xlsx_path = Some(xlsx_path);
        outcome.html_path = Some(html_path);
    }

    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::{IndexAddOption, Repository, Signature};
    use std::fs;

    fn setup_repo() -> (PathBuf, String) {
        let tmp = std::env::temp_dir().join(format!("driftpatch_cli_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&tmp).unwrap();
        let repo = Repository::init(&tmp).unwrap();

        let file = tmp.join("src").join("Foo.java");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "class Foo {}\n").unwrap();
        commit_all(&repo, "init");

        fs::write(&file, "class Foo { void bar() {} }\n").unwrap();
        let oid = commit_all(&repo, "add method");
        (tmp.clone(), oid.to_string())
    }

    fn commit_all(repo: &Repository, message: &str) -> git2::Oid {
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"].iter(), IndexAddOption::DEFAULT, None)
            .unwrap();
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
    }

    #[test]
    fn test_cli_import_from_commit() {
        let (repo_path, commit_sha) = setup_repo();
        let patch_repo = repo_path.join("patch-repo");
        fs::create_dir_all(&patch_repo).unwrap();

        let outcome = import_from_commit(&FromCommitConfig {
            repo: repo_path.clone(),
            commit: commit_sha,
            work_dir: repo_path.clone(),
            patch_repo: patch_repo.clone(),
            author: "tester".to_string(),
            description: Some("cli test".to_string()),
            report_dir: None,
        })
        .unwrap();

        assert!(outcome.saved >= 1);
        assert_eq!(outcome.failed, 0);

        let patches = PatchRepository::new(&patch_repo).list().unwrap();
        assert!(!patches.is_empty());

        let _ = fs::remove_dir_all(&repo_path);
    }
}
