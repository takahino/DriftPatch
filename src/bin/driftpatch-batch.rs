use clap::{Parser, Subcommand};
use std::path::PathBuf;

use driftpatch::batch::{
    apply_all, check_patches, import_from_commit, BatchApplyConfig, FromCommitConfig,
    PatchCheckConfig,
};
use driftpatch::i18n::{lang_from_str, set_lang, tr, tr_args};

// clap の help 属性は Command 構築時（parse 時）に評価されるため、
// 環境変数 DRIFTPATCH_LANG を parse 前に反映すればヘルプも切り替わる。
// --lang フラグは parse 後にしか分からないため、ランタイムメッセージにのみ効く。
#[derive(Parser)]
#[command(name = "driftpatch-batch")]
#[command(about = tr("cli.about"))]
struct Cli {
    /// メッセージ言語 (ja / en)
    #[arg(long, global = true, help = tr("cli.lang_help"))]
    lang: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = tr("cli.apply.about"))]
    Apply {
        #[arg(long, help = tr("cli.apply.workdir"))]
        workdir: PathBuf,
        #[arg(long, help = tr("cli.apply.patch_dir"))]
        patch_dir: PathBuf,
        #[arg(long, help = tr("cli.apply.report_dir"))]
        report_dir: PathBuf,
        #[arg(long, help = tr("cli.apply.dry_run"))]
        dry_run: bool,
    },
    #[command(about = tr("cli.fc.about"))]
    FromCommit {
        #[arg(long, help = tr("cli.fc.repo"))]
        repo: PathBuf,
        #[arg(long, help = tr("cli.fc.commit"))]
        commit: String,
        #[arg(long, help = tr("cli.fc.workdir"))]
        workdir: PathBuf,
        #[arg(long, help = tr("cli.fc.patch_repo"))]
        patch_repo: PathBuf,
        #[arg(long, help = tr("cli.fc.author"))]
        author: Option<String>,
        #[arg(long, help = tr("cli.fc.description"))]
        description: Option<String>,
        #[arg(long, help = tr("cli.fc.report_dir"))]
        report_dir: Option<PathBuf>,
    },
    #[command(about = tr("cli.check.about"))]
    Check {
        #[arg(long, help = tr("cli.check.patch_dir"))]
        patch_dir: PathBuf,
    },
}

fn main() {
    // ヘルプ文言にも効かせるため、環境変数の言語指定は parse 前に反映する
    if let Ok(env_lang) = std::env::var("DRIFTPATCH_LANG") {
        if let Some(l) = lang_from_str(&env_lang) {
            set_lang(l);
        }
    }

    // profiles.json（カスタム言語プロファイル）を読み込む。失敗しても続行する
    if let Some(warning) = driftpatch::lexer::custom::init_custom_profiles() {
        eprintln!("{}", warning);
    }

    let cli = Cli::parse();

    // --lang は環境変数より優先する
    if let Some(ref lang_str) = cli.lang {
        if let Some(l) = lang_from_str(lang_str) {
            set_lang(l);
        }
    }

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
                        println!("{}", tr("cli.apply.dry_run_done"));
                    } else {
                        println!("{}", tr("cli.apply.done"));
                    }
                    println!(
                        "{}",
                        tr_args(
                            "cli.summary_line",
                            &[
                                ("total", &outcome.report.summary.total.to_string()),
                                ("success", &outcome.report.summary.success.to_string()),
                                ("skipped", &outcome.report.summary.skipped.to_string()),
                                ("failed", &outcome.report.summary.failed.to_string()),
                            ]
                        )
                    );
                    println!("  Excel: {}", outcome.xlsx_path.display());
                    println!("  HTML:  {}", outcome.html_path.display());

                    if outcome.report.summary.failed > 0 {
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("{}", tr_args("cli.error", &[("err", &e)]));
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
                    println!("{}", tr("cli.fc.done"));
                    println!(
                        "{}",
                        tr_args(
                            "cli.fc.summary",
                            &[
                                ("saved", &outcome.saved.to_string()),
                                ("skipped", &outcome.skipped.to_string()),
                                ("failed", &outcome.failed.to_string()),
                            ]
                        )
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
                    eprintln!("{}", tr_args("cli.error", &[("err", &e)]));
                    std::process::exit(1);
                }
            }
        }
        Commands::Check { patch_dir } => {
            let config = PatchCheckConfig { patch_dir };

            match check_patches(&config) {
                Ok(outcome) => {
                    let errors: Vec<_> = outcome.errors().collect();
                    let warnings: Vec<_> = outcome.warnings().collect();

                    if errors.is_empty() && warnings.is_empty() {
                        println!(
                            "{}",
                            tr_args("cli.check.ok", &[("dir", &outcome.patch_dir)])
                        );
                    } else {
                        if !warnings.is_empty() {
                            println!(
                                "{}",
                                tr_args(
                                    "cli.check.warnings",
                                    &[("count", &warnings.len().to_string())]
                                )
                            );
                            for f in &warnings {
                                println!("  - {}", f.describe());
                            }
                        }
                        if !errors.is_empty() {
                            println!(
                                "{}",
                                tr_args(
                                    "cli.check.errors",
                                    &[("count", &errors.len().to_string())]
                                )
                            );
                            for f in &errors {
                                println!("  - {}", f.describe());
                            }
                        }
                    }

                    if !errors.is_empty() {
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("{}", tr_args("cli.error", &[("err", &e)]));
                    std::process::exit(1);
                }
            }
        }
    }
}
