use crate::app::DriftPatchApp;

/// 下部パネル: パッチ一覧テーブル
pub fn render_patch_panel(app: &mut DriftPatchApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.strong("パッチ一覧");
        if ui.button("🔄 更新").clicked() {
            app.reload_patches();
        }
        if app.file_path.is_some() {
            // 適用・削除ボタン（パッチが選択されている場合のみ有効）
            let can_act = app.selected_patch.is_some();
            ui.add_enabled_ui(can_act, |ui| {
                if ui.button("▶ 適用").clicked() {
                    app.apply_selected_patch();
                }
                if ui.button("🗑 削除").clicked() {
                    app.delete_selected_patch();
                }
            });
        }
    });

    ui.separator();

    if app.patches.is_empty() {
        if app.settings.patch_repo_path.is_empty() {
            ui.label("⚠ パッチリポジトリパスが設定されていません（設定ボタンから設定してください）");
        } else {
            ui.label("パッチがありません");
        }
        return;
    }

    let mut selection_changed = false;
    let mut new_selection: Option<usize> = app.selected_patch;

    egui::ScrollArea::vertical()
        .id_salt("patch_list")
        .max_height(120.0)
        .show(ui, |ui| {
            egui::Grid::new("patch_grid")
                .striped(true)
                .min_col_width(80.0)
                .show(ui, |ui| {
                    // ヘッダー行
                    ui.strong("ファイル名");
                    ui.strong("作者");
                    ui.strong("説明");
                    ui.strong("作成日時");
                    ui.end_row();

                    for (idx, (filename, patch)) in app.patches.iter().enumerate() {
                        let selected = app.selected_patch == Some(idx);
                        let resp = ui.selectable_label(selected, filename.as_str());
                        if resp.clicked() {
                            new_selection = Some(idx);
                            selection_changed = true;
                        }
                        ui.label(patch.author.as_str());
                        ui.label(patch.description.as_str());
                        ui.label(patch.created_at.as_str());
                        ui.end_row();
                    }
                });
        });

    if selection_changed {
        app.selected_patch = new_selection;
        app.update_preview();
    }
}
