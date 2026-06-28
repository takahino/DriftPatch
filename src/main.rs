mod app;
mod diff;
mod encoding;
mod lexer;
mod patch;
mod ui;

use app::DriftPatchApp;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("DriftPatch")
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([800.0, 500.0]),
        ..Default::default()
    };

    eframe::run_native(
        "DriftPatch",
        native_options,
        Box::new(|cc| {
            // 日本語フォントをロード
            setup_fonts(&cc.egui_ctx);
            Ok(Box::new(DriftPatchApp::default()))
        }),
    )
}

/// 日本語フォントを egui に登録する。
/// Windows の日本語フォントを試み、見つかれば Proportional/Monospace に追加する。
fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    let font_candidates = [
        "C:/Windows/Fonts/YuGothM.ttc",
        "C:/Windows/Fonts/msgothic.ttc",
        "C:/Windows/Fonts/meiryo.ttc",
    ];

    for font_path in &font_candidates {
        if let Ok(data) = std::fs::read(font_path) {
            fonts.font_data.insert(
                "japanese".to_owned(),
                egui::FontData::from_owned(data).into(),
            );
            fonts
                .families
                .get_mut(&egui::FontFamily::Proportional)
                .unwrap()
                .push("japanese".to_owned());
            fonts
                .families
                .get_mut(&egui::FontFamily::Monospace)
                .unwrap()
                .push("japanese".to_owned());
            break;
        }
    }

    ctx.set_fonts(fonts);
}
