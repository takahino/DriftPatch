use driftpatch::diff::line_diff::overlaps_range;
use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, TextBuffer};
use egui_code_editor::{format_token, ColorTheme, Syntax, Token, TokenType};

/// フォントサイズ設定の許容範囲（settings_window の DragValue と共有）
pub const MIN_FONT_SIZE: f32 = 10.0;
pub const MAX_FONT_SIZE: f32 = 24.0;
pub const DEFAULT_FONT_SIZE: f32 = 13.0;

const ROWS: usize = 40;

/// 差分ハイライト付きコードエディタを表示する。
#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    id: &str,
    text: &mut String,
    syntax: &Syntax,
    theme: ColorTheme,
    highlight_ranges: &[(usize, usize)],
    highlight_color: Color32,
    editable: bool,
    font_size: f32,
) {
    let frame = egui::Frame::new().fill(theme.bg());
    frame.show(ui, |ui| {
        ui.horizontal_top(|ui| {
            theme.modify_style(ui, font_size);
            show_numlines(ui, id, text.as_str(), theme, font_size);
            egui::ScrollArea::horizontal()
                .id_salt(format!("{id}_inner_scroll"))
                .show(ui, |ui| {
                    let mut layouter =
                        |ui: &egui::Ui, text_buffer: &dyn TextBuffer, _wrap_width: f32| {
                            let layout_job = build_layout_job(
                                text_buffer.as_str(),
                                syntax,
                                &theme,
                                highlight_ranges,
                                highlight_color,
                                font_size,
                            );
                            ui.fonts_mut(|f| f.layout_job(layout_job))
                        };

                    let mut text_edit = egui::TextEdit::multiline(text)
                        .id_salt(id)
                        .font(egui::TextStyle::Monospace)
                        .desired_rows(ROWS)
                        .desired_width(f32::INFINITY)
                        .layouter(&mut layouter);

                    if !editable {
                        text_edit = text_edit.interactive(false);
                    }

                    text_edit.show(ui);
                });
        });
    });
}

fn build_layout_job(
    text: &str,
    syntax: &Syntax,
    theme: &ColorTheme,
    highlight_ranges: &[(usize, usize)],
    highlight_color: Color32,
    font_size: f32,
) -> LayoutJob {
    let mut job = LayoutJob::default();
    let mut lexer = Token::default();
    let mut byte_offset = 0usize;

    for token in lexer.tokens(syntax, text) {
        let buf = token.buffer();
        if buf.is_empty() {
            continue;
        }

        let start = byte_offset;
        let end = byte_offset + buf.len();
        byte_offset = end;

        let mut format = format_token(theme, font_size, token.ty());
        if overlaps_range(start, end, highlight_ranges) {
            format.background = highlight_color;
        }
        job.append(buf, 0.0, format);
    }

    // 末尾に改行がない場合でもレイアウトが崩れないよう、残りを Unknown として追加
    if byte_offset < text.len() {
        let remainder = &text[byte_offset..];
        let mut format = format_token(theme, font_size, TokenType::Unknown);
        if overlaps_range(byte_offset, text.len(), highlight_ranges) {
            format.background = highlight_color;
        }
        job.append(remainder, 0.0, format);
    }

    job
}

fn show_numlines(ui: &mut egui::Ui, id: &str, text: &str, theme: ColorTheme, font_size: f32) {
    let total = if text.ends_with('\n') || text.is_empty() {
        text.lines().count() + 1
    } else {
        text.lines().count()
    }
    .max(ROWS) as isize;

    let max_indent = total.to_string().len();
    let mut counter = (1..=total)
        .map(|i| {
            let label = i.to_string();
            format!(
                "{}{label}",
                " ".repeat(max_indent.saturating_sub(label.len()))
            )
        })
        .collect::<Vec<String>>()
        .join("\n");

    #[allow(clippy::cast_precision_loss)]
    let width = max_indent as f32 * font_size * 0.5;

    let mut layouter = |ui: &egui::Ui, text_buffer: &dyn TextBuffer, _wrap_width: f32| {
        let layout_job = LayoutJob::single_section(
            text_buffer.as_str().to_string(),
            TextFormat::simple(
                egui::FontId::monospace(font_size),
                theme.type_color(TokenType::Comment(true)),
            ),
        );
        ui.fonts_mut(|f| f.layout_job(layout_job))
    };

    ui.add(
        egui::TextEdit::multiline(&mut counter)
            .id_salt(format!("{id}_numlines"))
            .font(egui::TextStyle::Monospace)
            .interactive(false)
            .frame(egui::Frame::NONE)
            .desired_rows(ROWS)
            .desired_width(width)
            .layouter(&mut layouter),
    );
}
