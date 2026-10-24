//! Scheduling engine for Sheafly.
//!
//! Instants are Unix milliseconds holding project wall-clock time (formatted as UTC).
//! Durations and lags are working minutes. Calendars convert between the two.
//! The engine is the source of truth: the UI sends commands and renders the result.

pub mod assignment_matrix;
pub mod autosave;
pub mod availability;
pub mod baseline_load;
pub mod calendar;
pub mod calendar_base;
pub mod checkpoint;
pub mod clipboard;
pub mod command;
pub mod curves;
pub mod document;
pub mod error;
pub mod evm;
pub mod fields;
pub mod focus;
pub mod forecast;
mod graph;
pub mod import;
pub mod lang;
pub mod level;
mod level_cycle;
pub mod level_order;
pub mod load_map;
pub mod model;
pub mod mspdi;
mod mspdi_availability;
mod mspdi_calendar_days;
mod mspdi_cost;
mod mspdi_fields;
mod mspdi_progress;
mod mspdi_range;
mod mspdi_skills;
pub mod mspdi_report;
mod mspdi_workweeks;
pub mod network;
pub mod plan;
pub mod pull;
pub mod push;
pub mod recovery;
pub mod recurrence;
mod ripple;
pub mod schedule;
pub mod skill_levels;
pub mod skills;
pub mod source;
pub mod split;
pub mod statistics;
pub mod suggest;
pub mod store;
pub mod sync;
pub mod timeutil;
pub mod tracking;
pub mod work;

pub use calendar::Calendar;
pub use command::Command;
pub use document::{Document, Snapshot};
pub use error::{EngineError, Invalid};
pub use lang::Lang;
pub use model::{
    Accrual, Assignment, ConstraintType, DepKind, Dependency, Overallocation, Project, Resource,
    ResourceKind, Task, TaskType, UsageDay, UsageRow,
};
pub use schedule::schedule;
