use crate::app::DriftPatchApp;

/// 設定ウィンドウを描画する
pub fn render_settings_window(app: &mut DriftPatchApp, ctx: &egui::Context) {
    if !app.show_settings {
        return;
    }

    let mut open = app.show_settings;
    let mut save_and_close = false;

    egui::Window::new("設定")
        .open(&mut open)
        .resizable(true)
        .default_width(400.0)
        .show(ctx, |ui| {
            egui::Grid::new("settings_grid")
                .num_columns(2)
                .spacing([10.0, 8.0])
                .show(ui, |ui| {
                    ui.label("ユーザー名:");
                    ui.text_edit_singleline(&mut app.settings.username);
                    ui.end_row();

                    ui.label("パッチリポジトリパス:");
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut app.settings.patch_repo_path);
                        if ui.button("📂").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .set_title("パッチリポジトリフォルダを選択")
                                .pick_folder()
                            {
                                app.settings.patch_repo_path =
                                    path.to_str().unwrap_or("").to_string();
                            }
                        }
                    });
                    ui.end_row();

                    ui.label("Git リポジトリパス:");
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut app.settings.git_repo_path);
                        if ui.button("📂").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .set_title("Git リポジトリフォルダを選択")
                                .pick_folder()
                            {
                                app.settings.git_repo_path =
                                    path.to_str().unwrap_or("").to_string();
                            }
                        }
                    });
                    ui.end_row();

                    ui.label("work ディレクトリ:");
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut app.settings.work_dir);
                        if ui.button("📂").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .set_title("work ディレクトリを選択")
                                .pick_folder()
                            {
                                app.settings.work_dir =
                                    path.to_str().unwrap_or("").to_string();
                            }
                        }
                    });
                    ui.end_row();

                    ui.label("適用時に.bak作成:");
                    ui.checkbox(&mut app.settings.create_backup, "有効");
                    ui.end_row();
                });

            ui.separator();

            if ui.button("保存して閉じる").clicked() {
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
