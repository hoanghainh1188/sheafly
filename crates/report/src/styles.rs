//! Cell formats. Workbooks have no theme, so the colours are fixed and match the app's
//! light palette. Amounts carry no currency sign: a project has no currency, and the app
//! shows plain grouped numbers too.

use rust_xlsxwriter::{Color, Format, FormatAlign, FormatBorder};

const HEADER_FILL: u32 = 0x1E3A8A;
const HEADER_TEXT: u32 = 0xFFFFFF;
const HEADER_BORDER: u32 = 0xCBD5E1;
const GRID: u32 = 0xE2E8F0;
const GROUP_FILL: u32 = 0xF1F5F9;
const TOTAL_FILL: u32 = 0xE2E8F0;
const TOTAL_BORDER: u32 = 0x94A3B8;
const OVER_TEXT: u32 = 0xB91C1C;
const OVER_FILL: u32 = 0xFEE2E2;
const OVER_BORDER: u32 = 0xFCA5A5;

const DATE: &str = "yyyy-mm-dd hh:mm";
const HOURS: &str = "#,##0.0";
const MONEY: &str = "#,##0";
const PERCENT: &str = "0%";

/// One kind of cell, as it looks on a task row and on a group (summary) row.
pub(crate) struct Pair {
    pub(crate) task: Format,
    pub(crate) group: Format,
}

impl Pair {
    pub(crate) fn of(&self, summary: bool) -> &Format {
        if summary { &self.group } else { &self.task }
    }

    fn new(base: Format) -> Self {
        let group = base.clone().set_bold().set_background_color(Color::RGB(GROUP_FILL));
        Self { task: base, group }
    }
}

pub(crate) struct Styles {
    pub(crate) header: Format,
    pub(crate) text: Pair,
    pub(crate) center: Pair,
    pub(crate) date: Pair,
    pub(crate) hours: Pair,
    pub(crate) money: Pair,
    pub(crate) percent: Pair,
    pub(crate) over: Format,
    pub(crate) total_label: Format,
    pub(crate) total_hours: Format,
}

impl Styles {
    pub(crate) fn new() -> Self {
        let cell = || Format::new().set_border(FormatBorder::Thin).set_border_color(Color::RGB(GRID));
        let total = Format::new()
            .set_bold()
            .set_background_color(Color::RGB(TOTAL_FILL))
            .set_align(FormatAlign::Right)
            .set_border(FormatBorder::Thin)
            .set_border_color(Color::RGB(TOTAL_BORDER));
        Self {
            header: Format::new()
                .set_bold()
                .set_text_wrap()
                .set_font_color(Color::RGB(HEADER_TEXT))
                .set_background_color(Color::RGB(HEADER_FILL))
                .set_align(FormatAlign::Center)
                .set_align(FormatAlign::VerticalCenter)
                .set_border(FormatBorder::Thin)
                .set_border_color(Color::RGB(HEADER_BORDER)),
            text: Pair::new(cell()),
            center: Pair::new(cell().set_align(FormatAlign::Center)),
            date: Pair::new(cell().set_num_format(DATE).set_align(FormatAlign::Center)),
            hours: Pair::new(cell().set_num_format(HOURS).set_align(FormatAlign::Right)),
            money: Pair::new(cell().set_num_format(MONEY).set_align(FormatAlign::Right)),
            percent: Pair::new(cell().set_num_format(PERCENT).set_align(FormatAlign::Right)),
            over: Format::new()
                .set_bold()
                .set_num_format(HOURS)
                .set_font_color(Color::RGB(OVER_TEXT))
                .set_background_color(Color::RGB(OVER_FILL))
                .set_align(FormatAlign::Right)
                .set_border(FormatBorder::Thin)
                .set_border_color(Color::RGB(OVER_BORDER)),
            total_label: total.clone(),
            total_hours: total.set_num_format(HOURS),
        }
    }
}
