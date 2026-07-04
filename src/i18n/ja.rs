//! 日本語カタログ（原文言語）。
//! 既存コードの文言をそのまま移してあるため、ここを変更すると
//! 既存テストの assert 文字列と食い違う可能性がある点に注意。

pub static CATALOG: &[(&str, &str)] = &[
    // --- パッチ適用（applier） ---
    (
        "apply.no_match",
        "ハンク {hunk} の適用箇所が見つかりませんでした",
    ),
    (
        "apply.count_mismatch",
        "ハンク {hunk} の期待マッチ数 {expected} と実際のマッチ数 {actual} が一致しません。位置: {positions}",
    ),
    (
        "apply.overlapping",
        "ハンク {hunk} の複数マッチの置換範囲が重なっています",
    ),
    // --- パッチ生成（generator） ---
    ("gen.no_diff", "変更が見つかりませんでした"),
    // --- 内容検証（verify） ---
    (
        "verify.mismatch",
        "期待トークン数 {expected} / 実際 {actual}, 最初の相違位置: {index}",
    ),
    // --- パッチ種別ラベル ---
    ("kind.modify", "変更"),
    ("kind.create", "新規作成"),
    ("kind.delete", "削除"),
    ("kind.rename", "リネーム"),
    // --- パッチ整合性検証（model） ---
    (
        "model.no_old_path_for_kind",
        "{kind}パッチに old_path は指定できません",
    ),
    (
        "model.delete_requires_verify",
        "削除パッチには verify_tokens（内容検証情報）が必要です",
    ),
    ("model.delete_no_hunks", "削除パッチに hunks は指定できません"),
    (
        "model.rename_requires_old_path",
        "リネームパッチには old_path が必要です",
    ),
    (
        "model.pure_rename_requires_verify",
        "内容変更のないリネームパッチには verify_tokens が必要です",
    ),
    // --- 実行操作（PlannedAction） ---
    ("action.modify", "適用成功"),
    ("action.create", "ファイル作成"),
    ("action.delete", "ファイル削除"),
    ("action.rename", "リネーム: {from} → {to}"),
    ("action.already_applied", "適用済み（変更なし）"),
    // --- ファイル操作（file_ops） ---
    ("fops.read_error", "ファイル読込エラー: {path}: {err}"),
    ("fops.write_error", "ファイル書込エラー: {path}: {err}"),
    ("fops.mkdir_error", "ディレクトリ作成エラー: {path}: {err}"),
    ("fops.delete_error", "ファイル削除エラー: {path}: {err}"),
    ("fops.rename_error", "リネームエラー: {from} → {to}: {err}"),
    ("fops.backup_error", "バックアップ作成失敗: {path}: {err}"),
    (
        "fops.target_deleted_earlier",
        "対象ファイルは先行パッチにより削除済みです: {path}",
    ),
    ("fops.target_not_found", "対象ファイルが見つかりません: {path}"),
    (
        "fops.already_exists",
        "作成先に異なる内容のファイルが既に存在します: {path}",
    ),
    (
        "fops.delete_verification_failed",
        "削除を中止しました。ファイル内容がパッチ記録時と一致しません（ドリフト検出）: {path} ({mismatch})",
    ),
    (
        "fops.rename_verification_failed",
        "リネームを中止しました。移動前ファイルの内容がパッチ記録時と一致しません（ドリフト検出）: {path} ({mismatch})",
    ),
    ("common.invalid_patch", "パッチが不正: {msg}"),
    (
        "fops.delete_missing_verify",
        "削除パッチに verify_tokens がありません",
    ),
    (
        "fops.rename_missing_old_path",
        "リネームパッチに old_path がありません",
    ),
    (
        "fops.rename_missing_verify",
        "リネームパッチに verify_tokens がありません",
    ),
    // --- パッチリポジトリ（repository） ---
    ("repo.io", "I/Oエラー: {err}"),
    ("repo.json", "JSONエラー: {err}"),
    ("repo.invalid_path", "パスが無効: {path}"),
    (
        "repo.unsupported_version",
        "未対応のパッチフォーマットバージョンです: {version}（新しい DriftPatch で作成された可能性があります）",
    ),
    ("common.empty_target", "target_file が空です"),
    // --- バッチ適用（batch） ---
    ("batch.list_error", "パッチ列挙エラー: {err}"),
    (
        "batch.report_dir_error",
        "レポートディレクトリ作成エラー: {err}",
    ),
    ("batch.xlsx_error", "Excel レポート出力エラー: {err}"),
    ("batch.html_error", "HTML レポート出力エラー: {err}"),
    // --- 競合チェック（check） ---
    (
        "check.overlapping_hunk",
        "重複ハンク: {patch_a} のハンク {hunk_a} と {patch_b} のハンク {hunk_b} が同一ファイル {target} の重なる範囲を触っています",
    ),
    (
        "check.modify_deleted",
        "削除対象への編集: {edit_patch} は {delete_patch} により削除される {target} を編集しようとしています",
    ),
    (
        "check.rename_old_path",
        "リネーム旧パス宛パッチ: {patch} は {rename_patch} の旧パス {old_path} を対象としています（適用順序に依存）",
    ),
    // --- レポート ---
    ("report.title", "DriftPatch パッチ適用レポート"),
    ("report.dryrun_xlsx", "DRY-RUN（ファイルは変更されていません）"),
    (
        "report.dryrun_html",
        "DRY-RUN: 適用可否の判定のみ行いました。ファイルは変更されていません。",
    ),
    ("report.summary", "サマリ"),
    ("report.h.patch_path", "パッチパス"),
    ("report.h.patch_id", "パッチID"),
    ("report.h.target", "対象ファイル"),
    ("report.h.status", "ステータス"),
    ("report.h.action", "操作"),
    ("report.h.error_kind", "エラー種別"),
    ("report.h.hunk", "ハンク番号"),
    ("report.h.message", "メッセージ"),
    ("report.h.started", "開始時刻"),
    ("report.h.finished", "終了時刻"),
    ("report.h.start_short", "開始"),
    ("report.h.end_short", "終了"),
    ("report.total", "合計"),
    ("report.success", "成功"),
    ("report.failed", "失敗"),
    // --- Git コミット取り込み ---
    ("fc.saved", "生成・保存成功"),
    ("git.not_a_repo", "Git リポジトリではありません"),
    ("git.invalid_commit", "無効なコミット: {commit}"),
    ("git.git2", "Git エラー: {err}"),
    ("git.io", "I/O エラー: {err}"),
    ("git.skip_binary", "バイナリファイルはスキップ"),
    ("git.no_rename_old", "リネーム元パスが取得できません"),
    (
        "git.no_after_content",
        "コミット後のファイル内容が取得できません",
    ),
    (
        "git.no_parent_for_delete",
        "親コミットがないため削除前の内容を取得できません",
    ),
    (
        "git.no_before_content",
        "削除前のファイル内容が取得できません",
    ),
    (
        "git.no_parent_for_rename",
        "親コミットがないためリネーム元の内容を取得できません",
    ),
    (
        "git.no_rename_before",
        "リネーム前のファイル内容が取得できません",
    ),
    (
        "git.no_rename_after",
        "リネーム後のファイル内容が取得できません",
    ),
    ("git.empty_path", "パスが空です"),
    ("git.path_outside", "不正なパス（work_dir 外参照）: {path}"),
    (
        "git.not_under_workdir_missing",
        "work_dir 配下にファイルがありません: {path} (work_dir: {work_dir})",
    ),
    (
        "git.not_under_workdir",
        "対象ファイルが work_dir 配下にありません: {path}",
    ),
    // --- CLI（driftpatch-batch） ---
    ("cli.about", "DriftPatch 一括パッチ適用 CLI"),
    (
        "cli.lang_help",
        "メッセージ言語 (ja / en)。環境変数 DRIFTPATCH_LANG でも指定可",
    ),
    (
        "cli.apply.about",
        "workdir と patch dir を指定してパッチを一括適用する",
    ),
    ("cli.apply.workdir", "修正対象ファイルのワークディレクトリ"),
    (
        "cli.apply.patch_dir",
        "パッチが格納されたディレクトリ（patches/ または repo ルート）",
    ),
    ("cli.apply.report_dir", "レポート出力先ディレクトリ"),
    (
        "cli.apply.dry_run",
        "ファイルを変更せず、適用可否と予定操作のみレポートする",
    ),
    ("cli.fc.about", "Git コミットから .dpatch を一括生成する"),
    ("cli.fc.repo", "Git リポジトリパス"),
    ("cli.fc.commit", "コミット SHA または ref"),
    ("cli.fc.workdir", "target_file 相対化の基準ディレクトリ"),
    ("cli.fc.patch_repo", "パッチリポジトリルート（patches/ の親）"),
    ("cli.fc.author", "パッチ作者名"),
    (
        "cli.fc.description",
        "パッチ説明（未指定時はコミットメッセージ）",
    ),
    ("cli.fc.report_dir", "レポート出力先ディレクトリ（任意）"),
    (
        "cli.check.about",
        "patch-dir 内のパッチ同士の両立性を適用せずに検査する",
    ),
    (
        "cli.check.patch_dir",
        "パッチが格納されたディレクトリ（patches/ または repo ルート）",
    ),
    (
        "cli.apply.dry_run_done",
        "dry-run 完了（ファイルは変更されていません）",
    ),
    ("cli.apply.done", "一括適用完了"),
    (
        "cli.summary_line",
        "  合計: {total} / 成功: {success} / 失敗: {failed}",
    ),
    ("cli.error", "エラー: {err}"),
    ("cli.fc.done", "Git コミットからのパッチ生成完了"),
    (
        "cli.fc.summary",
        "  保存: {saved} / スキップ: {skipped} / 失敗: {failed}",
    ),
    ("cli.check.ok", "OK: 競合は検出されませんでした ({dir})"),
    ("cli.check.warnings", "警告 ({count} 件):"),
    ("cli.check.errors", "競合 ({count} 件):"),
    // --- GUI ---
    ("gui.open_prompt", "ファイルを開いてください"),
    (
        "gui.opened_file",
        "開いたファイル: {file} | 言語: {lang} | エンコード: {enc}",
    ),
    ("gui.open_error", "ファイルオープンエラー: {err}"),
    ("gui.patch_list_error", "パッチ一覧読み込みエラー: {err}"),
    (
        "gui.workdir_not_set_check",
        "work_dir が設定されていません（設定を確認してください）",
    ),
    ("gui.workdir_not_set", "work_dir が設定されていません"),
    ("gui.patch_no_target", "パッチに target_file がありません"),
    ("gui.no_file_open", "ファイルが開かれていません"),
    (
        "gui.repo_not_set_check",
        "パッチリポジトリパスが設定されていません（設定を確認してください）",
    ),
    (
        "gui.repo_not_set",
        "パッチリポジトリパスが設定されていません",
    ),
    ("gui.patch_saved", "パッチ保存: {path}"),
    ("gui.patch_save_error", "パッチ保存エラー: {err}"),
    (
        "gui.patch_not_for_open_file",
        "選択したパッチは現在開いているファイル向けではありません",
    ),
    (
        "gui.delete_preview_ok",
        "削除パッチ: 適用するとこのファイルは削除されます（内容検証 OK）",
    ),
    (
        "gui.delete_preview_drift",
        "削除パッチ: 内容がパッチ記録時と一致しません（ドリフト検出）: {mismatch}",
    ),
    (
        "gui.delete_preview_invalid",
        "削除パッチ: verify_tokens がありません（不正なパッチ）",
    ),
    (
        "gui.create_preview",
        "新規作成パッチ: 適用すると {path} が作成されます",
    ),
    ("gui.preview_failed", "プレビュー失敗: {err}"),
    ("gui.rename_preview", "リネームパッチ: {from} → {to}"),
    ("gui.preview_updated", "プレビュー更新完了"),
    (
        "gui.apply_fail_no_match",
        "適用失敗: ハンク {hunk} の対象箇所が見つかりません",
    ),
    (
        "gui.apply_fail_count",
        "適用失敗: ハンク {hunk} の期待マッチ数 {expected} と実際のマッチ数 {actual} が一致しません（ドリフト検出）。",
    ),
    (
        "gui.apply_fail_overlap",
        "適用失敗: ハンク {hunk} の複数マッチの置換範囲が重なっています。",
    ),
    (
        "gui.rename_needs_workdir",
        "リネームパッチの適用には work_dir の設定が必要です",
    ),
    ("gui.rename_applied", "リネーム適用完了: {from} → {to}"),
    ("gui.rename_patch_status", "リネームパッチ: {desc}"),
    ("gui.apply_error", "パッチ適用エラー: {err}"),
    (
        "gui.applied_with_backup",
        "パッチ適用完了: {path} に保存、バックアップ: {bak}",
    ),
    ("gui.applied", "パッチ適用完了: {path} に保存"),
    (
        "gui.delete_applied_with_backup",
        "削除パッチ適用完了: {path} を削除、バックアップ: {bak}",
    ),
    ("gui.delete_applied", "削除パッチ適用完了: {path} を削除"),
    ("gui.apply_status", "パッチ適用: {desc}"),
    (
        "gui.git_repo_not_set",
        "Git リポジトリパスまたは work_dir が設定されていません",
    ),
    ("gui.git_history_error", "Git 履歴読み込みエラー: {err}"),
    (
        "gui.git_import_done",
        "Git 取り込み完了: {saved} 件保存, {skipped} 件スキップ",
    ),
    (
        "gui.git_import_partial",
        "Git 取り込み: {saved} 件保存, {skipped} 件スキップ, {failed} 件保存失敗",
    ),
    ("gui.git_import_skipped_header", "スキップ:"),
    ("gui.git_import_more", "... 他 {count} 件"),
    ("gui.git_import_save_errors", "保存エラー:"),
    ("gui.git_import_error", "Git 取り込みエラー: {err}"),
    ("gui.patch_deleted", "削除: {path}"),
    ("gui.patch_delete_error", "削除エラー: {err}"),
    ("gui.btn_open", "📂 ファイルを開く"),
    ("gui.dlg_open_title", "ファイルを開く"),
    ("gui.btn_git_import", "📜 Git コミットから取り込み"),
    ("gui.btn_generate", "🔧 パッチ生成..."),
    ("gui.btn_settings", "⚙ 設定"),
    ("gui.status_file", "📄 {file}  |  言語: {lang}  |  ENC: {enc}"),
    ("gui.win_generate", "パッチ生成"),
    (
        "gui.generate_desc_label",
        "パッチの説明（Gitリポジトリの要件番号など）:",
    ),
    ("gui.btn_do_generate", "生成"),
    ("gui.btn_cancel", "キャンセル"),
    ("gui.panel_patches", "パッチ一覧"),
    ("gui.btn_refresh", "🔄 更新"),
    ("gui.btn_apply", "▶ 適用"),
    ("gui.btn_delete", "🗑 削除"),
    (
        "gui.warn_repo_not_set",
        "⚠ パッチリポジトリパスが設定されていません（設定ボタンから設定してください）",
    ),
    (
        "gui.hint_open_file",
        "ファイルを開くと、そのファイル向けのパッチが表示されます",
    ),
    (
        "gui.warn_workdir_not_set",
        "⚠ work_dir が設定されていません（設定ボタンから設定してください）",
    ),
    (
        "gui.warn_not_under_workdir",
        "⚠ 開いているファイルが work_dir 配下にありません",
    ),
    (
        "gui.no_patches_for_file",
        "このファイル向けのパッチがありません: {path}",
    ),
    ("gui.target_label", "対象: {path}"),
    ("gui.col_patch", "パッチ"),
    ("gui.col_kind", "種別"),
    ("gui.col_author", "作者"),
    ("gui.col_desc", "説明"),
    ("gui.col_created", "作成日時"),
    ("gui.win_settings", "設定"),
    ("gui.set_username", "ユーザー名:"),
    ("gui.set_repo_path", "パッチリポジトリパス:"),
    ("gui.pick_repo", "パッチリポジトリフォルダを選択"),
    ("gui.set_git_path", "Git リポジトリパス:"),
    ("gui.pick_git", "Git リポジトリフォルダを選択"),
    ("gui.set_workdir", "work ディレクトリ:"),
    ("gui.pick_workdir", "work ディレクトリを選択"),
    ("gui.set_backup", "適用時に.bak作成:"),
    ("gui.enabled", "有効"),
    ("gui.btn_save_close", "保存して閉じる"),
    ("gui.set_language", "言語 / Language:"),
    ("gui.win_git_import", "Git コミットからパッチ生成"),
    (
        "gui.git_select_hint",
        "コミットを選択するか、SHA / ref を直接入力してください。",
    ),
    ("gui.git_sha_label", "コミット SHA / ref:"),
    (
        "gui.git_desc_label",
        "説明（空欄の場合はコミットメッセージを使用）:",
    ),
    ("gui.git_recent", "最近のコミット:"),
    ("gui.git_sha_required", "コミット SHA を指定してください"),
    ("gui.col_original", "修正前（読取専用）"),
    ("gui.col_editable", "修正画面（編集可）"),
    ("gui.col_preview", "パッチ適用プレビュー"),
];
