use crate::app::DriftPatchApp;
use driftpatch::i18n::{tr, tr_args};
use egui::Context;

const COLOR_SUCCESS: egui::Color32 = egui::Color32::from_rgb(46, 125, 50);
const COLOR_SKIPPED: egui::Color32 = egui::Color32::from_rgb(21, 101, 192);
const COLOR_FAILED: egui::Color32 = egui::Color32::from_rgb(198, 40, 40);
const COLOR_WARNING: egui::Color32 = egui::Color32::from_rgb(245, 124, 0);

/// GUI からの一括適用・dry-run・競合チェックダイアログ
pub fn render_batch_window(app: &mut DriftPatchApp, ctx: &Context) {
    if app.batch_dialog.is_none() {
        return;
    }

    let mut open = true;
    let mut do_apply = false;
    let mut do_check = false;
    let mut close = false;

    egui::Window::new(tr("gui.win_batch"))
        .open(&mut open)
        .resizable(true)
        .default_width(640.0)
        .default_height(480.0)
        .show(ctx, |ui| {
            let dialog = app.batch_dialog.as_mut().unwrap();

            egui::Grid::new("batch_grid")
                .num_columns(2)
                .spacing([10.0, 8.0])
                .show(ui, |ui| {
                    ui.label(tr("gui.set_workdir"));
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut dialog.work_dir);
                        if ui.button("📂").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .set_title(tr("gui.pick_workdir"))
                                .pick_folder()
                            {
                                dialog.work_dir = path.to_str().unwrap_or("").to_string();
                            }
                        }
                    });
                    ui.end_row();

                    ui.label(tr("gui.batch_patch_dir"));
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut dialog.patch_dir);
                        if ui.button("📂").clicked() {
                            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                dialog.patch_dir = path.to_str().unwrap_or("").to_string();
                            }
                        }
                    });
                    ui.end_row();

                    ui.label(tr("gui.batch_report_dir"));
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut dialog.report_dir);
                        if ui.button("📂").clicked() {
                            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                dialog.report_dir = path.to_str().unwrap_or("").to_string();
                            }
                        }
                    });
                    ui.end_row();

                    ui.label(tr("gui.batch_dry_run"));
                    ui.checkbox(&mut dialog.dry_run, tr("gui.enabled"));
                    ui.end_row();
                });

            ui.separator();

            ui.horizontal(|ui| {
                let apply_label = if dialog.dry_run {
                    tr("gui.batch_btn_dry_run")
                } else {
                    tr("gui.batch_btn_apply")
                };
                if ui.button(apply_label).clicked() {
                    do_apply = true;
                }
                if ui.button(tr("gui.batch_btn_check")).clicked() {
                    do_check = true;
                }
            });

            if !dialog.dry_run {
                ui.colored_label(COLOR_WARNING, tr("gui.batch_apply_warning"));
            }

            if let Some(ref err) = dialog.error.clone() {
                ui.colored_label(egui::Color32::RED, format!("❌ {}", err));
            }

            ui.separator();

            if let Some(ref outcome) = dialog.apply_outcome {
                ui.strong(tr("gui.batch_apply_result"));
                ui.label(tr_args(
                    "gui.batch_apply_summary",
                    &[
                        ("total", &outcome.report.summary.total.to_string()),
                        ("success", &outcome.report.summary.success.to_string()),
                        ("skipped", &outcome.report.summary.skipped.to_string()),
                        ("failed", &outcome.report.summary.failed.to_string()),
                    ],
                ));
                ui.label(format!("Excel: {}", outcome.xlsx_path.display()));
                ui.label(format!("HTML:  {}", outcome.html_path.display()));

                egui::ScrollArea::vertical()
                    .id_salt("batch_apply_rows")
                    .max_height(200.0)
                    .show(ui, |ui| {
                        egui::Grid::new("batch_apply_grid")
                            .striped(true)
                            .show(ui, |ui| {
                                for row in &outcome.report.rows {
                                    let color = match row.status.as_str() {
                                        "success" => COLOR_SUCCESS,
                                        "skipped" => COLOR_SKIPPED,
                                        _ => COLOR_FAILED,
                                    };
                                    ui.colored_label(color, &row.status);
                                    ui.label(&row.target_file);
                                    ui.label(&row.message);
                                    ui.end_row();
                                }
                            });
                    });
            }

            if let Some(ref outcome) = dialog.check_outcome {
                ui.separator();
                ui.strong(tr("gui.batch_check_result"));
                let errors: Vec<_> = outcome.errors().collect();
                let warnings: Vec<_> = outcome.warnings().collect();
                if errors.is_empty() && warnings.is_empty() {
                    ui.colored_label(COLOR_SUCCESS, tr("gui.batch_check_ok"));
                } else {
                    for w in &warnings {
                        ui.colored_label(COLOR_WARNING, format!("⚠ {}", w.describe()));
                    }
                    for e in &errors {
                        ui.colored_label(COLOR_FAILED, format!("❌ {}", e.describe()));
                    }
                }
            }
        });

    if !open || close {
        app.batch_dialog = None;
        return;
    }

    if do_apply {
        app.run_batch_apply();
    }
    if do_check {
        app.run_patch_check();
    }
}
