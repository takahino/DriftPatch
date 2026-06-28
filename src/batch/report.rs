use std::path::Path;

use rust_xlsxwriter::{Format, Workbook};

#[derive(Debug, Clone)]
pub struct ReportSummary {
    pub total: usize,
    pub success: usize,
    pub failed: usize,
}

#[derive(Debug, Clone)]
pub struct ReportRow {
    pub patch_path: String,
    pub patch_id: String,
    pub target_file: String,
    pub status: String,
    pub error_kind: Option<String>,
    pub hunk_index: Option<usize>,
    pub message: String,
    pub started_at: String,
    pub finished_at: String,
}

#[derive(Debug, Clone)]
pub struct BatchReport {
    pub work_dir: String,
    pub patch_dir: String,
    pub started_at: String,
    pub finished_at: String,
    pub summary: ReportSummary,
    pub rows: Vec<ReportRow>,
}

pub fn write_xlsx_report(report: &BatchReport, path: &Path) -> Result<(), String> {
    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();

    worksheet
        .set_name("apply_report")
        .map_err(|e| e.to_string())?;

    let header = Format::new().set_bold();
    let headers = [
        "パッチパス",
        "パッチID",
        "対象ファイル",
        "ステータス",
        "エラー種別",
        "ハンク番号",
        "メッセージ",
        "開始時刻",
        "終了時刻",
    ];

    for (col, title) in headers.iter().enumerate() {
        worksheet
            .write_string_with_format(0, col as u16, *title, &header)
            .map_err(|e| e.to_string())?;
    }

    for (row_idx, row) in report.rows.iter().enumerate() {
        let r = (row_idx + 1) as u32;
        worksheet
            .write_string(r, 0, &row.patch_path)
            .map_err(|e| e.to_string())?;
        worksheet
            .write_string(r, 1, &row.patch_id)
            .map_err(|e| e.to_string())?;
        worksheet
            .write_string(r, 2, &row.target_file)
            .map_err(|e| e.to_string())?;
        worksheet
            .write_string(r, 3, &row.status)
            .map_err(|e| e.to_string())?;
        worksheet
            .write_string(r, 4, row.error_kind.as_deref().unwrap_or(""))
            .map_err(|e| e.to_string())?;
        if let Some(hunk) = row.hunk_index {
            worksheet
                .write_number(r, 5, hunk as f64)
                .map_err(|e| e.to_string())?;
        } else {
            worksheet.write_string(r, 5, "").map_err(|e| e.to_string())?;
        }
        worksheet
            .write_string(r, 6, &row.message)
            .map_err(|e| e.to_string())?;
        worksheet
            .write_string(r, 7, &row.started_at)
            .map_err(|e| e.to_string())?;
        worksheet
            .write_string(r, 8, &row.finished_at)
            .map_err(|e| e.to_string())?;
    }

    let summary_row = (report.rows.len() + 2) as u32;
    worksheet
        .write_string(summary_row, 0, "サマリ")
        .map_err(|e| e.to_string())?;
    worksheet
        .write_string(
            summary_row,
            1,
            &format!(
                "total={} success={} failed={}",
                report.summary.total, report.summary.success, report.summary.failed
            ),
        )
        .map_err(|e| e.to_string())?;

    workbook.save(path).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn write_html_report(report: &BatchReport, path: &Path) -> Result<(), String> {
    let mut html = String::new();
    html.push_str("<!DOCTYPE html>\n<html lang=\"ja\">\n<head>\n");
    html.push_str("<meta charset=\"UTF-8\">\n");
    html.push_str("<title>DriftPatch Apply Report</title>\n");
    html.push_str("<style>\n");
    html.push_str("body { font-family: sans-serif; margin: 24px; }\n");
    html.push_str("table { border-collapse: collapse; width: 100%; }\n");
    html.push_str("th, td { border: 1px solid #ccc; padding: 8px; text-align: left; }\n");
    html.push_str("th { background: #f0f0f0; }\n");
    html.push_str(".success { background: #e8f5e9; }\n");
    html.push_str(".failed { background: #ffebee; }\n");
    html.push_str(".summary { margin-bottom: 16px; }\n");
    html.push_str("</style>\n</head>\n<body>\n");
    html.push_str("<h1>DriftPatch パッチ適用レポート</h1>\n");
    html.push_str("<div class=\"summary\">\n");
    html.push_str(&format!("<p><strong>work_dir:</strong> {}</p>\n", html_escape(&report.work_dir)));
    html.push_str(&format!("<p><strong>patch_dir:</strong> {}</p>\n", html_escape(&report.patch_dir)));
    html.push_str(&format!("<p><strong>開始:</strong> {} / <strong>終了:</strong> {}</p>\n", report.started_at, report.finished_at));
    html.push_str(&format!(
        "<p><strong>合計:</strong> {} / <strong>成功:</strong> {} / <strong>失敗:</strong> {}</p>\n",
        report.summary.total, report.summary.success, report.summary.failed
    ));
    html.push_str("</div>\n");
    html.push_str("<table>\n<thead><tr>\n");
    for h in [
        "パッチパス",
        "パッチID",
        "対象ファイル",
        "ステータス",
        "エラー種別",
        "ハンク番号",
        "メッセージ",
        "開始",
        "終了",
    ] {
        html.push_str(&format!("<th>{}</th>\n", h));
    }
    html.push_str("</tr></thead>\n<tbody>\n");

    for row in &report.rows {
        let row_class = if row.status == "success" {
            "success"
        } else {
            "failed"
        };
        html.push_str(&format!("<tr class=\"{}\">\n", row_class));
        html.push_str(&format!("<td>{}</td>\n", html_escape(&row.patch_path)));
        html.push_str(&format!("<td>{}</td>\n", html_escape(&row.patch_id)));
        html.push_str(&format!("<td>{}</td>\n", html_escape(&row.target_file)));
        html.push_str(&format!("<td>{}</td>\n", html_escape(&row.status)));
        html.push_str(&format!(
            "<td>{}</td>\n",
            html_escape(row.error_kind.as_deref().unwrap_or(""))
        ));
        html.push_str(&format!(
            "<td>{}</td>\n",
            row.hunk_index
                .map(|h| h.to_string())
                .unwrap_or_else(|| String::new())
        ));
        html.push_str(&format!("<td>{}</td>\n", html_escape(&row.message)));
        html.push_str(&format!("<td>{}</td>\n", html_escape(&row.started_at)));
        html.push_str(&format!("<td>{}</td>\n", html_escape(&row.finished_at)));
        html.push_str("</tr>\n");
    }

    html.push_str("</tbody></table>\n</body></html>\n");
    std::fs::write(path, html.as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod report_tests {
    use super::*;

    fn sample_report() -> BatchReport {
        BatchReport {
            work_dir: "C:/work".to_string(),
            patch_dir: "C:/patches".to_string(),
            started_at: "2026-06-28 10:00:00".to_string(),
            finished_at: "2026-06-28 10:00:01".to_string(),
            summary: ReportSummary {
                total: 2,
                success: 1,
                failed: 1,
            },
            rows: vec![
                ReportRow {
                    patch_path: "src/Foo.java/a.dpatch".to_string(),
                    patch_id: "id1".to_string(),
                    target_file: "src/Foo.java".to_string(),
                    status: "success".to_string(),
                    error_kind: None,
                    hunk_index: None,
                    message: "適用成功".to_string(),
                    started_at: "2026-06-28 10:00:00".to_string(),
                    finished_at: "2026-06-28 10:00:00".to_string(),
                },
                ReportRow {
                    patch_path: "src/Bar.java/b.dpatch".to_string(),
                    patch_id: "id2".to_string(),
                    target_file: "src/Bar.java".to_string(),
                    status: "failed".to_string(),
                    error_kind: Some("NoMatch".to_string()),
                    hunk_index: Some(0),
                    message: "not found".to_string(),
                    started_at: "2026-06-28 10:00:01".to_string(),
                    finished_at: "2026-06-28 10:00:01".to_string(),
                },
            ],
        }
    }

    #[test]
    fn test_write_reports() {
        let tmp = std::env::temp_dir().join(format!("driftpatch_report_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        let report = sample_report();
        let xlsx = tmp.join("report.xlsx");
        let html = tmp.join("report.html");

        write_xlsx_report(&report, &xlsx).unwrap();
        write_html_report(&report, &html).unwrap();

        assert!(xlsx.exists());
        assert!(html.exists());
        let html_text = std::fs::read_to_string(&html).unwrap();
        assert!(html_text.contains("src/Foo.java/a.dpatch"));
        assert!(html_text.contains("failed"));

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
