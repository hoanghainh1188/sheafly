//! The public functions of the Sheafly Pro crate, with their exact types (plan part 20G).
//! This file is the same in the public stub (`crates/pro/tests/api.rs`) and in the private
//! crate (`scripts/check-stub.py` compares them), so each CI compiles it against its own
//! crate and a signature that drifts fails to build. Add a line with every new function.

use serde_json::Value;
use sheafly_engine::advice::AdviceStep;
use sheafly_engine::health::HealthReport;
use sheafly_engine::model::Project;
use sheafly_engine::summary::ProjectSummary;
use sheafly_engine::Lang;

#[test]
fn the_public_functions_keep_their_types() {
    let _: fn() -> bool = sheafly_pro::available;
    let _: fn(Lang) -> String = sheafly_pro::unavailable;
    let _: fn(&Project, Lang) -> Result<Vec<u8>, String> = sheafly_pro::excel_workbook;
    let _: fn(&Project, Lang) -> Result<HealthReport, String> = sheafly_pro::schedule_health;
    let _: fn(&ProjectSummary, &str, Lang) -> Result<Value, String> = sheafly_pro::risks_request;
    let _: fn(&Value, &Value, &Project, bool, Lang) -> Result<AdviceStep, String> = sheafly_pro::risks_next;
}
