mod app;
mod ui;

use app::DriftPatchApp;

fn main() -> eframe::Result<()> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("DriftPatch")
        .with_inner_size([1200.0, 800.0])
        .with_min_inner_size([800.0, 500.0]);

    if let Some(icon) = load_icon() {
        viewport = viewport.with_icon(icon);
    }

    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "DriftPatch",
        native_options,
        Box::new(|cc| {
            setup_fonts(&cc.egui_ctx);
            // profiles.json（カスタム言語プロファイル）を DriftPatchApp 生成前に読み込む。
            // 失敗しても起動は止めず、警告を初期ステータスに表示する
            let profile_warning = driftpatch::lexer::custom::init_custom_profiles();
            let mut app = DriftPatchApp::default();
            if let Some(warning) = profile_warning {
                app.status_message = warning;
            }
            Ok(Box::new(app))
        }),
    )
}

/// `icon.png` を読み込んでウィンドウアイコン用の RGBA データに変換する。
fn load_icon() -> Option<egui::IconData> {
    let bytes = include_bytes!("../icon.png");
    let img = image::load_from_memory(bytes).ok()?.into_rgba8();
    let (width, height) = img.dimensions();
    Some(egui::IconData {
        rgba: img.into_raw(),
        width,
        height,
    })
}

/// 日本語フォントを egui に登録する。
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
