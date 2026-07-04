use crate::app::{find_matches, line_of_byte, DriftPatchApp};
use driftpatch::diff::inline_diff;
use driftpatch::i18n::tr;
use egui_code_editor::{ColorTheme, Syntax};

use super::diff_editor;

const REMOVED_COLOR: egui::Color32 = egui::Color32::from_rgb(80, 0, 0);
const ADDED_COLOR: egui::Color32 = egui::Color32::from_rgb(0, 80, 0);

/// 3列エディタレイアウトを描画する。
/// 左列: 元テキスト（読取専用）
/// 中列: 編集用エディタ（スクロールオフセット更新元、Ctrl+F 検索対応）
/// 右列: パッチ適用プレビュー（読取専用）
pub fn render_editors(app: &mut DriftPatchApp, ui: &mut egui::Ui) {
    handle_search_shortcuts(app, ui);

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
                ui.label(tr("gui.col_original"));
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
                            app.settings.font_size,
                            &[],
                            None,
                        );
                    });
            });
        });

        ui.separator();

        // ---- 中列: 編集用エディタ ----
        ui.allocate_ui(egui::vec2(col_width, available.y), |ui| {
            ui.vertical(|ui| {
                if app.search.open {
                    render_search_bar(app, ui);
                } else {
                    ui.label(tr("gui.col_editable"));
                }

                let mut scroll_area = egui::ScrollArea::vertical().id_salt("editor_center");
                if let Some(target) = app.search.scroll_target.take() {
                    scroll_area = scroll_area.vertical_scroll_offset(target);
                }

                // edited_text を &mut で渡すため、matches は複製して借用の競合を避ける
                let search_ranges = app.search.matches.clone();
                let current_match = app.search.matches.get(app.search.current).copied();

                let scroll_resp = scroll_area.show(ui, |ui| {
                    diff_editor::show(
                        ui,
                        "code_center",
                        &mut app.edited_text,
                        &syntax,
                        theme,
                        &added_ranges,
                        ADDED_COLOR,
                        true,
                        app.settings.font_size,
                        &search_ranges,
                        current_match,
                    );
                });
                app.scroll_offset = scroll_resp.state.offset.y;
            });
        });

        ui.separator();

        // ---- 右列: パッチ適用プレビュー（読取専用）----
        ui.allocate_ui(egui::vec2(col_width, available.y), |ui| {
            ui.vertical(|ui| {
                ui.label(tr("gui.col_preview"));
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
                            app.settings.font_size,
                            &[],
                            None,
                        );
                    });
            });
        });
    });
}

/// Ctrl+F / Esc / Enter(F3) / Shift+Enter(Shift+F3) を処理する。
/// TextEdit にフォーカスを奪われないよう、エディタ本体の描画より前に呼ぶ。
fn handle_search_shortcuts(app: &mut DriftPatchApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let mut goto_next = false;
    let mut goto_prev = false;

    ctx.input_mut(|i| {
        if i.consume_key(egui::Modifiers::COMMAND, egui::Key::F) {
            app.search.open = true;
            app.search.focus_requested = true;
        }
        if app.search.open {
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                app.search.open = false;
                app.search.matches.clear();
            }
            if i.consume_key(egui::Modifiers::SHIFT, egui::Key::Enter)
                || i.consume_key(egui::Modifiers::SHIFT, egui::Key::F3)
            {
                goto_prev = true;
            } else if i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                || i.consume_key(egui::Modifiers::NONE, egui::Key::F3)
            {
                goto_next = true;
            }
        }
    });

    if goto_next {
        step_match(app, ui, 1);
    }
    if goto_prev {
        step_match(app, ui, -1);
    }
}

/// 現在のマッチから `delta` 件先（負なら前）へ循環移動し、ジャンプを要求する
fn step_match(app: &mut DriftPatchApp, ui: &egui::Ui, delta: isize) {
    let len = app.search.matches.len();
    if len == 0 {
        return;
    }
    let cur = app.search.current as isize;
    let len_i = len as isize;
    app.search.current = (((cur + delta) % len_i + len_i) % len_i) as usize;
    request_jump(app, ui);
}

/// 現在のマッチ位置へスクロールする 1 フレーム限りの要求をセットする
fn request_jump(app: &mut DriftPatchApp, ui: &egui::Ui) {
    let Some(&(start, _)) = app.search.matches.get(app.search.current) else {
        return;
    };
    let line = line_of_byte(&app.edited_text, start);
    let row_height =
        ui.fonts_mut(|f| f.row_height(&egui::FontId::monospace(app.settings.font_size)));
    #[allow(clippy::cast_precision_loss)]
    let target = (line as f32 * row_height - ui.available_height() * 0.4).max(0.0);
    app.search.scroll_target = Some(target);
}

/// 中央列ヘッダーを検索バーに置き換えて表示する
fn render_search_bar(app: &mut DriftPatchApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        let response = ui.text_edit_singleline(&mut app.search.query);
        if app.search.focus_requested {
            response.request_focus();
            app.search.focus_requested = false;
        }
        if response.changed() {
            recompute_matches(app, ui);
        }

        let count_label = if app.search.matches.is_empty() {
            "0/0".to_string()
        } else {
            format!("{}/{}", app.search.current + 1, app.search.matches.len())
        };
        ui.label(count_label);

        if ui.button("↑").clicked() {
            step_match(app, ui, -1);
        }
        if ui.button("↓").clicked() {
            step_match(app, ui, 1);
        }

        let mut case_sensitive = app.search.case_sensitive;
        if ui.checkbox(&mut case_sensitive, "Aa").changed() {
            app.search.case_sensitive = case_sensitive;
            recompute_matches(app, ui);
        }

        if ui.button("✕").clicked() {
            app.search.open = false;
            app.search.matches.clear();
        }
    });
}

/// クエリ・大文字小文字設定変更時にマッチを再計算し、先頭マッチへジャンプする
fn recompute_matches(app: &mut DriftPatchApp, ui: &egui::Ui) {
    app.search.matches = find_matches(
        &app.edited_text,
        &app.search.query,
        app.search.case_sensitive,
    );
    app.search.current = 0;
    if !app.search.matches.is_empty() {
        request_jump(app, ui);
    }
}

fn lang_to_syntax(lang: &str) -> Syntax {
    match lang {
        "python" => Syntax::python(),
        "sql" | "plsql" => Syntax::sql(),
        _ => Syntax::rust(),
    }
}
