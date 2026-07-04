use crate::app::DriftPatchApp;
use driftpatch::i18n::{self, tr};

/// 設定ウィンドウを描画する
pub fn render_settings_window(app: &mut DriftPatchApp, ctx: &egui::Context) {
    if !app.show_settings {
        return;
    }

    let mut open = app.show_settings;
    let mut save_and_close = false;

    egui::Window::new(tr("gui.win_settings"))
        .open(&mut open)
        .resizable(true)
        .default_width(400.0)
        .show(ctx, |ui| {
            egui::Grid::new("settings_grid")
                .num_columns(2)
                .spacing([10.0, 8.0])
                .show(ui, |ui| {
                    ui.label(tr("gui.set_username"));
                    ui.text_edit_singleline(&mut app.settings.username);
                    ui.end_row();

                    ui.label(tr("gui.set_repo_path"));
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut app.settings.patch_repo_path);
                        if ui.button("📂").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .set_title(tr("gui.pick_repo"))
                                .pick_folder()
                            {
                                app.settings.patch_repo_path =
                                    path.to_str().unwrap_or("").to_string();
                            }
                        }
                    });
                    ui.end_row();

                    ui.label(tr("gui.set_git_path"));
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut app.settings.git_repo_path);
                        if ui.button("📂").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .set_title(tr("gui.pick_git"))
                                .pick_folder()
                            {
                                app.settings.git_repo_path =
                                    path.to_str().unwrap_or("").to_string();
                            }
                        }
                    });
                    ui.end_row();

                    ui.label(tr("gui.set_workdir"));
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut app.settings.work_dir);
                        if ui.button("📂").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .set_title(tr("gui.pick_workdir"))
                                .pick_folder()
                            {
                                app.settings.work_dir = path.to_str().unwrap_or("").to_string();
                            }
                        }
                    });
                    ui.end_row();

                    ui.label(tr("gui.set_backup"));
                    ui.checkbox(&mut app.settings.create_backup, tr("gui.enabled"));
                    ui.end_row();

                    ui.label(tr("gui.set_language"));
                    egui::ComboBox::from_id_salt("ui_language")
                        .selected_text(match app.settings.ui_language.as_str() {
                            "en" => "English",
                            _ => "日本語",
                        })
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_value(
                                    &mut app.settings.ui_language,
                                    "ja".to_string(),
                                    "日本語",
                                )
                                .clicked()
                            {
                                i18n::set_lang(i18n::Lang::Ja);
                            }
                            if ui
                                .selectable_value(
                                    &mut app.settings.ui_language,
                                    "en".to_string(),
                                    "English",
                                )
                                .clicked()
                            {
                                i18n::set_lang(i18n::Lang::En);
                            }
                        });
                    ui.end_row();
                });

            ui.separator();

            if ui.button(tr("gui.btn_save_close")).clicked() {
                app.settings.save();
                app.reload_patches();
                save_and_close = true;
            }
        });

    if save_and_close {
        open = false;
    }

    app.show_settings = open;
}
