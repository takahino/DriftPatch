use clap::{Parser, Subcommand};
use std::path::PathBuf;

use driftpatch::batch::{apply_all, BatchApplyConfig};

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
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Apply {
            workdir,
            patch_dir,
            report_dir,
        } => {
            let config = BatchApplyConfig {
                work_dir: workdir,
                patch_dir,
                report_dir,
            };

            match apply_all(&config) {
                Ok(outcome) => {
                    println!("一括適用完了");
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
    }
}
