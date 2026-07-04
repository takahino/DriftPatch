use crate::app::DriftPatchApp;
use driftpatch::i18n::{tr, tr_args};
use egui::Context;

/// ツールバーを描画する（ファイルを開く・パッチ生成・設定ボタン + ステータス表示）
pub fn render_toolbar(app: &mut DriftPatchApp, ctx: &Context, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        // ファイルを開くボタン
        if ui.button(tr("gui.btn_open")).clicked() {
            // rfd でネイティブファイル選択ダイアログを開く
            // ブロッキング版を使用（GUIスレッドでの呼び出し）
            if let Some(path) = rfd::FileDialog::new()
                .set_title(tr("gui.dlg_open_title"))
                .pick_file()
            {
                app.open_file(path);
            }
        }

        ui.separator();

        // Git コミットからパッチ生成
        if ui.button(tr("gui.btn_git_import")).clicked() {
            app.open_git_import();
        }

        ui.separator();

        // パッチ生成ボタン（ファイルが開かれている場合のみ有効）
        let can_generate = app.file_path.is_some();
        ui.add_enabled_ui(can_generate, |ui| {
            if ui.button(tr("gui.btn_generate")).clicked() {
                app.generate_patch_dialog = Some(crate::app::GeneratePatchDialog::default());
            }
        });

        ui.separator();

        // 設定ボタン
        if ui.button(tr("gui.btn_settings")).clicked() {
            app.show_settings = !app.show_settings;
        }

        ui.separator();

        // ステータス表示
        if let Some(ref path) = app.file_path {
            let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            ui.label(tr_args(
                "gui.status_file",
                &[
                    ("file", filename),
                    ("lang", app.language),
                    ("enc", &app.encoding),
                ],
            ));
        }
    });

    // パッチ生成ダイアログ
    render_generate_dialog(app, ctx);
}

fn render_generate_dialog(app: &mut DriftPatchApp, ctx: &Context) {
    if app.generate_patch_dialog.is_none() {
        return;
    }

    let mut open = true;
    let mut do_generate = false;
    let mut close = false;

    egui::Window::new(tr("gui.win_generate"))
        .open(&mut open)
        .resizable(false)
        .collapsible(false)
        .show(ctx, |ui| {
            let dialog = app.generate_patch_dialog.as_mut().unwrap();

            ui.label(tr("gui.generate_desc_label"));
            ui.text_edit_singleline(&mut dialog.description);

            if let Some(ref err) = dialog.error.clone() {
                ui.colored_label(egui::Color32::RED, format!("❌ {}", err));
            }
            if let Some(ref warn) = dialog.warning.clone() {
                ui.colored_label(egui::Color32::YELLOW, format!("⚠ {}", warn));
            }

            ui.separator();

            ui.horizontal(|ui| {
                if ui.button(tr("gui.btn_do_generate")).clicked() {
                    do_generate = true;
                }
                if ui.button(tr("gui.btn_cancel")).clicked() {
                    close = true;
                }
            });
        });

    if !open || close {
        app.generate_patch_dialog = None;
        return;
    }

    if do_generate {
        let description = app
            .generate_patch_dialog
            .as_ref()
            .map(|d| d.description.clone())
            .unwrap_or_default();

        match app.generate_and_save_patch(&description) {
            Ok(()) => {
                app.generate_patch_dialog = None;
            }
            Err(e) => {
                if let Some(ref mut dialog) = app.generate_patch_dialog {
                    dialog.error = Some(e);
                }
            }
        }
    }
}
