pub mod diff_editor;
mod editors;
mod git_import_window;
mod patch_panel;
mod settings_window;
mod toolbar;

use crate::app::DriftPatchApp;

/// メインの UI レンダリングエントリポイント（egui 0.35 の &mut Ui ベース API）
pub fn render(app: &mut DriftPatchApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();

    // 設定ウィンドウ（フローティング、Context ベース）
    settings_window::render_settings_window(app, &ctx);

    // Git コミット取り込みダイアログ
    git_import_window::render_git_import_window(app, &ctx);

    // ツールバー（上部パネル）
    egui::Panel::top("toolbar").resizable(false).show(ui, |ui| {
        toolbar::render_toolbar(app, &ctx, ui);
    });

    // ステータスバー（最下部）
    egui::Panel::bottom("status_bar")
        .resizable(false)
        .show(ui, |ui| {
            ui.label(&app.status_message);
        });

    // パッチ一覧パネル（下部、ステータスバーの上）
    egui::Panel::bottom("patch_panel")
        .resizable(true)
        .default_size(150.0)
        .show(ui, |ui| {
            patch_panel::render_patch_panel(app, ui);
        });

    // 3列エディタ（中央メインエリア）
    egui::CentralPanel::default().show(ui, |ui| {
        editors::render_editors(app, ui);
    });
}
