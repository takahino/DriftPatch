mod batch_window;
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

    handle_file_drop(app, &ctx, ui);

    // 設定ウィンドウ（フローティング、Context ベース）
    settings_window::render_settings_window(app, &ctx);

    // Git コミット取り込みダイアログ
    git_import_window::render_git_import_window(app, &ctx);

    // 一括適用・競合チェックダイアログ
    batch_window::render_batch_window(app, &ctx);

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

/// ファイルのドラッグ&ドロップで開く。ドロップ中はオーバーレイを表示する。
fn handle_file_drop(app: &mut DriftPatchApp, ctx: &egui::Context, ui: &mut egui::Ui) {
    let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
    if hovering {
        let rect = ui.max_rect();
        egui::Area::new(egui::Id::new("drop_overlay"))
            .fixed_pos(rect.min)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.painter()
                    .rect_filled(rect, 0.0, egui::Color32::from_black_alpha(160));
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    driftpatch::i18n::tr("gui.drop_to_open"),
                    egui::FontId::proportional(24.0),
                    egui::Color32::WHITE,
                );
            });
    }

    // dropped_files はこのフレーム限りなので、クロージャの外へ複製してから処理する
    let dropped: Vec<_> = ctx.input(|i| i.raw.dropped_files.clone());
    if let Some(path) = dropped.into_iter().find_map(|f| f.path) {
        app.open_file(path);
    }
}
