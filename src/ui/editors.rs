use crate::app::DriftPatchApp;
use egui_code_editor::{CodeEditor, ColorTheme, Syntax};

/// 3列エディタレイアウトを描画する。
/// 左列: 元テキスト（読取専用）
/// 中列: 編集用エディタ（スクロールオフセット更新元）
/// 右列: パッチ適用プレビュー（読取専用）
pub fn render_editors(app: &mut DriftPatchApp, ui: &mut egui::Ui) {
    let available = ui.available_size();
    let col_width = available.x / 3.0 - 4.0;

    let syntax = lang_to_syntax(app.language);

    ui.horizontal_top(|ui| {
        // ---- 左列: 元テキスト（読取専用）----
        ui.allocate_ui(egui::vec2(col_width, available.y), |ui| {
            ui.vertical(|ui| {
                ui.label("修正前（読取専用）");
                let scroll_resp = egui::ScrollArea::vertical()
                    .id_salt("editor_left")
                    .vertical_scroll_offset(app.scroll_offset)
                    .show(ui, |ui| {
                        let mut orig = app.original_text.clone();
                        let mut editor = CodeEditor::default()
                            .id_source("code_left")
                            .with_rows(40)
                            .with_fontsize(13.0)
                            .with_theme(ColorTheme::GITHUB_DARK)
                            .with_numlines(true)
                            .vscroll(false);
                        editor.show(ui, &mut orig, &syntax);
                    });
                let _ = scroll_resp;
            });
        });

        ui.separator();

        // ---- 中列: 編集用エディタ ----
        ui.allocate_ui(egui::vec2(col_width, available.y), |ui| {
            ui.vertical(|ui| {
                ui.label("修正画面（編集可）");
                let scroll_resp = egui::ScrollArea::vertical()
                    .id_salt("editor_center")
                    .show(ui, |ui| {
                        let mut editor = CodeEditor::default()
                            .id_source("code_center")
                            .with_rows(40)
                            .with_fontsize(13.0)
                            .with_theme(ColorTheme::GITHUB_DARK)
                            .with_numlines(true)
                            .vscroll(false);
                        editor.show(ui, &mut app.edited_text, &syntax);
                    });
                // 中列のスクロールオフセットを読み取って他の列に伝播させる
                app.scroll_offset = scroll_resp.state.offset.y;
            });
        });

        ui.separator();

        // ---- 右列: パッチ適用プレビュー（読取専用）----
        ui.allocate_ui(egui::vec2(col_width, available.y), |ui| {
            ui.vertical(|ui| {
                ui.label("パッチ適用プレビュー");
                egui::ScrollArea::vertical()
                    .id_salt("editor_right")
                    .vertical_scroll_offset(app.scroll_offset)
                    .show(ui, |ui| {
                        let mut preview = app.preview_text.clone();
                        let mut editor = CodeEditor::default()
                            .id_source("code_right")
                            .with_rows(40)
                            .with_fontsize(13.0)
                            .with_theme(ColorTheme::GITHUB_DARK)
                            .with_numlines(true)
                            .vscroll(false);
                        editor.show(ui, &mut preview, &syntax);
                    });
            });
        });
    });
}

fn lang_to_syntax(lang: &str) -> Syntax {
    match lang {
        "python" => Syntax::python(),
        "sql" => Syntax::sql(),
        // java, cpp, javascript, generic などは rust 構文でハイライト（最も近い）
        _ => Syntax::rust(),
    }
}
