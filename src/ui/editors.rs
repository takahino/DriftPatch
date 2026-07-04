use crate::app::DriftPatchApp;
use driftpatch::diff::inline_diff;
use egui_code_editor::{ColorTheme, Syntax};

use super::diff_editor;

const REMOVED_COLOR: egui::Color32 = egui::Color32::from_rgb(80, 0, 0);
const ADDED_COLOR: egui::Color32 = egui::Color32::from_rgb(0, 80, 0);

/// 3列エディタレイアウトを描画する。
/// 左列: 元テキスト（読取専用）
/// 中列: 編集用エディタ（スクロールオフセット更新元）
/// 右列: パッチ適用プレビュー（読取専用）
pub fn render_editors(app: &mut DriftPatchApp, ui: &mut egui::Ui) {
    let available = ui.available_size();
    let col_width = available.x / 3.0 - 4.0;

    let syntax = lang_to_syntax(app.language);
    let theme = ColorTheme::GITHUB_DARK;

    let (removed_ranges, added_ranges) = inline_diff(&app.original_text, &app.edited_text);
    let (_, preview_ranges) = if app.preview_text.is_empty() {
        (Vec::new(), Vec::new())
    } else {
        inline_diff(&app.original_text, &app.preview_text)
    };

    ui.horizontal_top(|ui| {
        // ---- 左列: 元テキスト（読取専用）----
        ui.allocate_ui(egui::vec2(col_width, available.y), |ui| {
            ui.vertical(|ui| {
                ui.label("修正前（読取専用）");
                let _scroll_resp = egui::ScrollArea::vertical()
                    .id_salt("editor_left")
                    .vertical_scroll_offset(app.scroll_offset)
                    .show(ui, |ui| {
                        let mut orig = app.original_text.clone();
                        diff_editor::show(
                            ui,
                            "code_left",
                            &mut orig,
                            &syntax,
                            theme,
                            &removed_ranges,
                            REMOVED_COLOR,
                            false,
                        );
                    });
            });
        });

        ui.separator();

        // ---- 中列: 編集用エディタ ----
        ui.allocate_ui(egui::vec2(col_width, available.y), |ui| {
            ui.vertical(|ui| {
                ui.label("修正画面（編集可）");
                let scroll_resp =
                    egui::ScrollArea::vertical()
                        .id_salt("editor_center")
                        .show(ui, |ui| {
                            diff_editor::show(
                                ui,
                                "code_center",
                                &mut app.edited_text,
                                &syntax,
                                theme,
                                &added_ranges,
                                ADDED_COLOR,
                                true,
                            );
                        });
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
                        diff_editor::show(
                            ui,
                            "code_right",
                            &mut preview,
                            &syntax,
                            theme,
                            &preview_ranges,
                            ADDED_COLOR,
                            false,
                        );
                    });
            });
        });
    });
}

fn lang_to_syntax(lang: &str) -> Syntax {
    match lang {
        "python" => Syntax::python(),
        "sql" | "plsql" => Syntax::sql(),
        _ => Syntax::rust(),
    }
}
