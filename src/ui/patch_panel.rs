use crate::app::DriftPatchApp;

/// 下部パネル: 開いているファイル向けパッチ一覧
pub fn render_patch_panel(app: &mut DriftPatchApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.strong("パッチ一覧");
        if ui.button("🔄 更新").clicked() {
            app.reload_patches();
        }
        if app.file_path.is_some() {
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

    if app.settings.patch_repo_path.is_empty() {
        ui.label("⚠ パッチリポジトリパスが設定されていません（設定ボタンから設定してください）");
        return;
    }

    if app.file_path.is_none() {
        ui.label("ファイルを開くと、そのファイル向けのパッチが表示されます");
        return;
    }

    if app.settings.work_dir.trim().is_empty() {
        ui.label("⚠ work_dir が設定されていません（設定ボタンから設定してください）");
        return;
    }

    if app.open_file_relative().is_none() {
        ui.label("⚠ 開いているファイルが work_dir 配下にありません");
        return;
    }

    let visible_patches = app.patches_for_open_file();
    if visible_patches.is_empty() {
        if let Some(rel) = app.open_file_relative() {
            ui.label(format!("このファイル向けのパッチがありません: {}", rel));
        }
        return;
    }

    let mut selection_changed = false;
    let mut new_selection: Option<String> = app.selected_patch.clone();

    if let Some(rel) = app.open_file_relative() {
        ui.label(format!("対象: {}", rel));
    }

    egui::ScrollArea::vertical()
        .id_salt("patch_list")
        .max_height(120.0)
        .show(ui, |ui| {
            egui::Grid::new("patch_grid")
                .striped(true)
                .min_col_width(80.0)
                .show(ui, |ui| {
                    ui.strong("パッチ");
                    ui.strong("種別");
                    ui.strong("作者");
                    ui.strong("説明");
                    ui.strong("作成日時");
                    ui.end_row();

                    for (patch_path, patch) in &visible_patches {
                        let selected = app.selected_patch.as_deref() == Some(patch_path.as_str());
                        let display_name =
                            patch_path.rsplit('/').next().unwrap_or(patch_path.as_str());
                        let resp = ui.selectable_label(selected, display_name);
                        if resp.clicked() {
                            new_selection = Some(patch_path.clone());
                            selection_changed = true;
                        }
                        ui.label(patch.kind.label());
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
