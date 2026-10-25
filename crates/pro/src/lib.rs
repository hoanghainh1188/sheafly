//! Stub of the Sheafly Pro modules (plan part 20G).
//!
//! The Pro features added since D1-B (the Excel workbook of sheafly-pro#3, the AI
//! recommendations of sheafly-pro#1, and later ones) are
//! closed source, in the private repository `sheafly-pro`. This crate has the same public
//! functions and the same dependencies, so the app builds from this repository alone;
//! each function answers that this build has no Pro modules. Release builds copy the
//! private crate over this directory. Add a function here, with the same signature,
//! whenever the private crate gains one, and keep the shared computations in the engine.

use serde_json::Value;
use sheafly_engine::advice::AdviceStep;
use sheafly_engine::health::HealthReport;
use sheafly_engine::model::Project;
use sheafly_engine::summary::ProjectSummary;
use sheafly_engine::Lang;

/// Whether this build has the Pro modules: never, for the stub.
pub fn available() -> bool {
    false
}

/// What every function of the stub answers, in `lang`.
pub fn unavailable(lang: Lang) -> String {
    match lang {
        Lang::Vi => "Bản Sheafly này không có các tính năng Pro (bản tự build từ mã nguồn công khai). Hãy dùng bản phát hành chính thức.".into(),
        Lang::En => "This Sheafly build has no Pro features (built from the public source). Use an official release.".into(),
    }
}

/// The project as an `.xlsx` workbook (sheafly-pro#3). The stub has none.
pub fn excel_workbook(_project: &Project, lang: Lang) -> Result<Vec<u8>, String> {
    Err(unavailable(lang))
}

/// The schedule health check, DCMA's 14 points (sheafly-pro#7). The stub has none.
pub fn schedule_health(_project: &Project, lang: Lang) -> Result<HealthReport, String> {
    Err(unavailable(lang))
}

/// The first request to explain a project's risks (sheafly-pro#1). The stub has none.
pub fn risks_request(_summary: &ProjectSummary, _model: &str, lang: Lang) -> Result<Value, String> {
    Err(unavailable(lang))
}

/// Read a reply to a request to explain risks. The stub has none.
pub fn risks_next(
    _body: &Value,
    _reply: &Value,
    _project: &Project,
    _mask: bool,
    lang: Lang,
) -> Result<AdviceStep, String> {
    Err(unavailable(lang))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sheafly_engine::Document;

    #[test]
    fn the_stub_has_no_pro_modules_and_says_so() {
        assert!(!available());
        let doc = Document::new_default();
        assert_eq!(excel_workbook(&doc.project, Lang::Vi), Err(unavailable(Lang::Vi)));
        assert_eq!(excel_workbook(&doc.project, Lang::En), Err(unavailable(Lang::En)));
        assert_eq!(schedule_health(&doc.project, Lang::En), Err(unavailable(Lang::En)));
        assert!(unavailable(Lang::En).starts_with("This Sheafly build"));
        let summary = sheafly_engine::summary::project_summary(&doc.project, &Default::default());
        assert_eq!(risks_request(&summary, "m", Lang::En), Err(unavailable(Lang::En)));
        let empty = serde_json::json!({});
        assert_eq!(risks_next(&empty, &empty, &doc.project, false, Lang::Vi), Err(unavailable(Lang::Vi)));
    }
}
