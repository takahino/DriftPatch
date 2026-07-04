use clap::{Parser, Subcommand};
use std::path::PathBuf;

use driftpatch::batch::{apply_all, import_from_commit, BatchApplyConfig, FromCommitConfig};

#[derive(Parser)]
#[command(name = "driftpatch-batch")]
#[command(about = "DriftPatch 一括パッチ適用 CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// workdir と patch dir を指定してパッチを一括適用する
    Apply {
        /// 修正対象ファイルのワークディレクトリ
        #[arg(long)]
        workdir: PathBuf,
        /// パッチが格納されたディレクトリ（patches/ または repo ルート）
        #[arg(long)]
        patch_dir: PathBuf,
        /// レポート出力先ディレクトリ
        #[arg(long)]
        report_dir: PathBuf,
        /// ファイルを変更せず、適用可否と予定操作のみレポートする
        #[arg(long)]
        dry_run: bool,
    },
    /// Git コミットから .dpatch を一括生成する
    FromCommit {
        /// Git リポジトリパス
        #[arg(long)]
        repo: PathBuf,
        /// コミット SHA または ref
        #[arg(long)]
        commit: String,
        /// target_file 相対化の基準ディレクトリ
        #[arg(long)]
        workdir: PathBuf,
        /// パッチリポジトリルート（patches/ の親）
        #[arg(long)]
        patch_repo: PathBuf,
        /// パッチ作者名
        #[arg(long)]
        author: Option<String>,
        /// パッチ説明（未指定時はコミットメッセージ）
        #[arg(long)]
        description: Option<String>,
        /// レポート出力先ディレクトリ（任意）
        #[arg(long)]
        report_dir: Option<PathBuf>,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Apply {
            workdir,
            patch_dir,
            report_dir,
            dry_run,
        } => {
            let config = BatchApplyConfig {
                work_dir: workdir,
                patch_dir,
                report_dir,
                dry_run,
            };

            match apply_all(&config) {
                Ok(outcome) => {
                    if dry_run {
                        println!("dry-run 完了（ファイルは変更されていません）");
                    } else {
                        println!("一括適用完了");
                    }
                    println!(
                        "  合計: {} / 成功: {} / 失敗: {}",
                        outcome.report.summary.total,
                        outcome.report.summary.success,
                        outcome.report.summary.failed
                    );
                    println!("  Excel: {}", outcome.xlsx_path.display());
                    println!("  HTML:  {}", outcome.html_path.display());

                    if outcome.report.summary.failed > 0 {
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("エラー: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::FromCommit {
            repo,
            commit,
            workdir,
            patch_repo,
            author,
            description,
            report_dir,
        } => {
            let config = FromCommitConfig {
                repo,
                commit,
                work_dir: workdir,
                patch_repo,
                author: author.unwrap_or_else(|| "unknown".to_string()),
                description,
                report_dir,
            };

            match import_from_commit(&config) {
                Ok(outcome) => {
                    println!("Git コミットからのパッチ生成完了");
                    println!(
                        "  保存: {} / スキップ: {} / 失敗: {}",
                        outcome.saved, outcome.skipped, outcome.failed
                    );
                    if let Some(ref xlsx) = outcome.xlsx_path {
                        println!("  Excel: {}", xlsx.display());
                    }
                    if let Some(ref html) = outcome.html_path {
                        println!("  HTML:  {}", html.display());
                    }

                    if outcome.failed > 0 || outcome.skipped > 0 {
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("エラー: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }
}
