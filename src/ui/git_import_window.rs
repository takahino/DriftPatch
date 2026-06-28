use crate::app::DriftPatchApp;
use egui::Context;

/// Git コミットからパッチを取り込むダイアログ
pub fn render_git_import_window(app: &mut DriftPatchApp, ctx: &Context) {
    if app.git_import_dialog.is_none() {
        return;
    }

    let mut open = true;
    let mut do_import = false;
    let mut close = false;
    let mut commit_sha = String::new();
    let mut description = String::new();

    egui::Window::new("Git コミットからパッチ生成")
        .open(&mut open)
        .resizable(true)
        .default_width(560.0)
        .default_height(420.0)
        .show(ctx, |ui| {
            let dialog = app.git_import_dialog.as_mut().unwrap();

            ui.label("コミットを選択するか、SHA / ref を直接入力してください。");

            ui.separator();

            ui.label("コミット SHA / ref:");
            ui.text_edit_singleline(&mut dialog.commit_input);

            ui.label("説明（空欄の場合はコミットメッセージを使用）:");
            ui.text_edit_singleline(&mut dialog.description);

            ui.separator();

            ui.strong("最近のコミット:");
            egui::ScrollArea::vertical()
                .id_salt("git_commit_list")
                .max_height(200.0)
                .show(ui, |ui| {
                    let commits: Vec<_> = dialog.commits.iter().enumerate().collect();
                    for (idx, commit) in commits {
                        let label = format!(
                            "{}  {}  {}  {}",
                            commit.short_sha, commit.time, commit.author, commit.summary
                        );
                        let selected = dialog.selected == Some(idx);
                        if ui.selectable_label(selected, label).clicked() {
                            dialog.selected = Some(idx);
                            dialog.commit_input = commit.sha.clone();
                            if dialog.description.is_empty() {
                                dialog.description = commit.summary.clone();
                            }
                        }
                    }
                });

            if let Some(ref err) = dialog.error.clone() {
                ui.colored_label(egui::Color32::RED, format!("❌ {}", err));
            }
            if let Some(ref msg) = dialog.result_message.clone() {
                ui.colored_label(egui::Color32::GREEN, format!("✅ {}", msg));
            }

            ui.separator();

            ui.horizontal(|ui| {
                if ui.button("生成").clicked() {
                    do_import = true;
                    commit_sha = dialog.commit_input.clone();
                    description = dialog.description.clone();
                }
                if ui.button("キャンセル").clicked() {
                    close = true;
                }
            });
        });

    if !open || close {
        app.git_import_dialog = None;
        return;
    }

    if do_import {
        if commit_sha.trim().is_empty() {
            if let Some(ref mut dialog) = app.git_import_dialog {
                dialog.error = Some("コミット SHA を指定してください".to_string());
            }
            return;
        }
        if let Some(ref mut dialog) = app.git_import_dialog {
            dialog.loading = true;
            dialog.error = None;
            dialog.result_message = None;
        }
        app.import_from_commit(&commit_sha, &description);
    }
}
