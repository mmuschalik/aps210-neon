//! The clause browser: one self-contained HTML file, data embedded as JSON.

use crate::Report;
use serde_json::json;

const PAGE: &str = include_str!("page.html");

/// `rev` names the spec version shown in the header (e.g. a git revision).
pub fn render(report: &Report, rev: &str) -> String {
    let mut data = report.to_json();
    data["meta"] = json!({ "rev": rev });
    // `</` would end the <script> element early.
    let data = data.to_string().replace("</", "<\\/");
    PAGE.replacen("/*DATA*/", &data, 1)
}
