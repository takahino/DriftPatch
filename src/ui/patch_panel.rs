use crate::app::DriftPatchApp;
use driftpatch::i18n::{tr, tr_args};

/// 下部パネル: 開いているファイル向けパッチ一覧
pub fn render_patch_panel(app: &mut DriftPatchApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.strong(tr("gui.panel_patches"));
        if ui.button(tr("gui.btn_refresh")).clicked() {
            app.reload_patches();
        }
        if app.file_path.is_some() {
            let can_act = app.selected_patch.is_some();
            ui.add_enabled_ui(can_act, |ui| {
                if ui.button(tr("gui.btn_apply")).clicked() {
                    app.apply_selected_patch();
                }
                if ui.button(tr("gui.btn_delete")).clicked() {
                    app.delete_selected_patch();
                }
            });
        }
    });

    ui.separator();

    if app.settings.patch_repo_path.is_empty() {
        ui.label(tr("gui.warn_repo_not_set"));
        return;
    }

    if app.file_path.is_none() {
        ui.label(tr("gui.hint_open_file"));
        return;
    }

    if app.settings.work_dir.trim().is_empty() {
        ui.label(tr("gui.warn_workdir_not_set"));
        return;
    }

    if app.open_file_relative().is_none() {
        ui.label(tr("gui.warn_not_under_workdir"));
        return;
    }

    let visible_patches = app.patches_for_open_file();
    if visible_patches.is_empty() {
        if let Some(rel) = app.open_file_relative() {
            ui.label(tr_args("gui.no_patches_for_file", &[("path", &rel)]));
        }
        return;
    }

    let mut selection_changed = false;
    let mut new_selection: Option<String> = app.selected_patch.clone();

    if let Some(rel) = app.open_file_relative() {
        ui.label(tr_args("gui.target_label", &[("path", &rel)]));
    }

    egui::ScrollArea::vertical()
        .id_salt("patch_list")
        .max_height(120.0)
        .show(ui, |ui| {
            egui::Grid::new("patch_grid")
                .striped(true)
                .min_col_width(80.0)
                .show(ui, |ui| {
                    ui.strong(tr("gui.col_patch"));
                    ui.strong(tr("gui.col_kind"));
                    ui.strong(tr("gui.col_author"));
                    ui.strong(tr("gui.col_desc"));
                    ui.strong(tr("gui.col_created"));
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
