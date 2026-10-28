//! Reading a table to import (phase 18 P3, step 3a): a CSV file or one sheet of an Excel or
//! OpenDocument workbook, as rows of text. The UI maps the columns and converts the cells;
//! this only turns the file into text the way it shows in a spreadsheet: whole numbers
//! without a decimal part, dates as ISO (`2026-01-05`, or `2026-01-05 08:00` with a time).

use std::path::Path;

use calamine::{open_workbook_auto, Data, Reader};
use serde::Serialize;

use sheafly_engine::Lang;

/// Rows a table can have, the header row aside; the engine imports at most this many.
pub const MAX_ROWS: usize = sheafly_engine::import::MAX_IMPORT_ROWS;
pub const MAX_COLUMNS: usize = 100;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Table {
    /// The workbook's sheets; empty for a CSV file.
    pub sheets: Vec<String>,
    /// The sheet read, if any.
    pub sheet: Option<String>,
    pub rows: Vec<Vec<String>>,
    /// The line or row number (from 1) of each row in the file, as a spreadsheet numbers
    /// it: blank rows and lines are left out of `rows`, so errors are shown by these.
    pub lines: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TableError {
    Read(String),
    /// Not UTF-8 nor UTF-16 with a byte order mark.
    Encoding,
    TooManyRows,
    TooManyColumns,
    NoSuchSheet,
}

impl TableError {
    pub fn message(&self, lang: Lang) -> String {
        match (self, lang) {
            (Self::Read(detail), Lang::Vi) => format!("Không đọc được bảng: {detail}"),
            (Self::Read(detail), Lang::En) => format!("Could not read the table: {detail}"),
            (Self::Read(detail), Lang::Ja) => format!("テーブルを読み込めませんでした: {detail}"),
            (Self::Encoding, Lang::Vi) => {
                "Tệp CSV không phải UTF-8. Hãy lưu lại dạng \"CSV UTF-8\" rồi nhập.".into()
            }
            (Self::Encoding, Lang::En) => {
                "The CSV file is not UTF-8. Save it as \"CSV UTF-8\" and import it again.".into()
            }
            (Self::Encoding, Lang::Ja) => {
                "CSV ファイルが UTF-8 ではありません。「CSV UTF-8」として保存してから再度インポートしてください。".into()
            }
            (Self::TooManyRows, Lang::Vi) => "Bảng có tối đa 20.000 dòng".into(),
            (Self::TooManyRows, Lang::En) => "A table can have at most 20,000 rows".into(),
            (Self::TooManyRows, Lang::Ja) => "テーブルの行数は最大 20,000 行までです".into(),
            (Self::TooManyColumns, Lang::Vi) => "Bảng có tối đa 100 cột".into(),
            (Self::TooManyColumns, Lang::En) => "A table can have at most 100 columns".into(),
            (Self::TooManyColumns, Lang::Ja) => "テーブルの列数は最大 100 列までです".into(),
            (Self::NoSuchSheet, Lang::Vi) => "Không có trang này trong tệp".into(),
            (Self::NoSuchSheet, Lang::En) => "The file has no such sheet".into(),
            (Self::NoSuchSheet, Lang::Ja) => "ファイル内に指定されたシートがありません".into(),
        }
    }
}

/// Whether `path` names a workbook rather than a CSV file.
fn is_workbook(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_lowercase();
    matches!(extension.as_str(), "xlsx" | "xlsm" | "xls" | "xlsb" | "ods")
}

/// Read `path`: the sheet `sheet` of a workbook (the first when `None`), or a CSV file.
pub fn read_table(path: &Path, sheet: Option<&str>) -> Result<Table, TableError> {
    if is_workbook(path) {
        read_workbook(path, sheet)
    } else {
        let bytes = std::fs::read(path).map_err(|error| TableError::Read(error.to_string()))?;
        let (rows, lines) = read_csv(&bytes)?;
        Ok(Table {
            sheets: Vec::new(),
            sheet: None,
            rows,
            lines,
        })
    }
}

fn read_workbook(path: &Path, sheet: Option<&str>) -> Result<Table, TableError> {
    let mut workbook = open_workbook_auto(path).map_err(|error| TableError::Read(error.to_string()))?;
    let sheets = workbook.sheet_names();
    let name = match sheet {
        Some(name) if sheets.iter().any(|sheet| sheet == name) => name.to_string(),
        Some(_) => return Err(TableError::NoSuchSheet),
        None => sheets.first().cloned().ok_or(TableError::NoSuchSheet)?,
    };
    let range = workbook
        .worksheet_range(&name)
        .map_err(|error| TableError::Read(error.to_string()))?;
    // The range starts at its first used cell; rows count from the sheet's top.
    let first = range.start().map_or(0, |(row, _)| row);
    let mut rows = Vec::new();
    let mut lines = Vec::new();
    for (index, cells) in range.rows().enumerate() {
        // A formatted but empty cell far to the right widens the range; count to the last value.
        let used = cells.iter().rposition(|cell| !matches!(cell, Data::Empty)).map_or(0, |last| last + 1);
        if used > MAX_COLUMNS {
            return Err(TableError::TooManyColumns);
        }
        if used == 0 {
            continue;
        }
        rows.push(cells[..used].iter().map(cell_text).collect());
        lines.push(first + index as u32 + 1);
        if rows.len() > MAX_ROWS + 1 {
            return Err(TableError::TooManyRows);
        }
    }
    let (rows, lines) = trimmed(rows, lines);
    Ok(Table {
        sheets,
        sheet: Some(name),
        rows,
        lines,
    })
}

fn cell_text(cell: &Data) -> String {
    match cell {
        Data::Empty | Data::Error(_) => String::new(),
        Data::String(text) | Data::DurationIso(text) => text.clone(),
        Data::DateTimeIso(text) => iso_text(text),
        Data::Int(number) => number.to_string(),
        Data::Float(number) => number_text(*number),
        Data::Bool(value) => value.to_string(),
        Data::DateTime(date) if date.is_duration() => hours_text(date.as_f64() * 24.0),
        Data::DateTime(date) => match date.as_datetime() {
            Some(moment) if moment.time() == chrono::NaiveTime::MIN => moment.format("%Y-%m-%d").to_string(),
            Some(moment) => moment.format("%Y-%m-%d %H:%M").to_string(),
            None => date.as_f64().to_string(),
        },
    }
}

/// A length of time in a cell (`[h]:mm`) as hours the UI reads as an effort: `16h`, `1.5h`.
fn hours_text(hours: f64) -> String {
    let minutes = (hours * 60.0).round();
    format!("{}h", number_text(minutes / 60.0))
}

/// An ISO date or time (OpenDocument writes these) in the form the other cells use:
/// `2026-01-09`, or `2026-01-09 13:30` with a time; anything else as it is.
fn iso_text(text: &str) -> String {
    let (date, time) = text.split_once('T').unwrap_or((text, ""));
    let time = time.get(..5).unwrap_or("");
    match (date.len(), time) {
        (10, "") | (10, "00:00") => date.to_string(),
        (10, time) => format!("{date} {time}"),
        _ => text.to_string(),
    }
}

/// A float as a spreadsheet shows it: `3` for a whole number, else the shortest digits.
fn number_text(number: f64) -> String {
    if number.fract() == 0.0 && number.abs() < 1e15 {
        format!("{}", number as i64)
    } else {
        number.to_string()
    }
}

/// Text of a CSV file: UTF-8 (with or without a byte order mark) or UTF-16 with one.
fn decoded(bytes: &[u8]) -> Result<String, TableError> {
    let utf16 = |bytes: &[u8], little: bool| {
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| if little { u16::from_le_bytes([pair[0], pair[1]]) } else { u16::from_be_bytes([pair[0], pair[1]]) })
            .collect();
        String::from_utf16(&units).map_err(|_| TableError::Encoding)
    };
    match bytes {
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8(rest.to_vec()).map_err(|_| TableError::Encoding),
        [0xFF, 0xFE, rest @ ..] if rest.len() % 2 == 0 => utf16(rest, true),
        [0xFE, 0xFF, rest @ ..] if rest.len() % 2 == 0 => utf16(rest, false),
        [0xFF, 0xFE, ..] | [0xFE, 0xFF, ..] => Err(TableError::Encoding),
        _ => String::from_utf8(bytes.to_vec()).map_err(|_| TableError::Encoding),
    }
}

/// The separator of the first line: whichever of comma, semicolon and tab it has most of
/// outside quotes (a comma when none). Spreadsheets in languages with a decimal comma
/// write semicolons.
fn delimiter(text: &str) -> u8 {
    let first = text.lines().next().unwrap_or("");
    let mut counts = [0usize; 3];
    let mut quoted = false;
    for character in first.chars() {
        match character {
            '"' => quoted = !quoted,
            ',' if !quoted => counts[0] += 1,
            ';' if !quoted => counts[1] += 1,
            '\t' if !quoted => counts[2] += 1,
            _ => {}
        }
    }
    let best = (0..3).max_by_key(|&index| (counts[index], 3 - index)).unwrap_or(0);
    if counts[best] == 0 {
        b','
    } else {
        b",;\t"[best]
    }
}

fn read_csv(bytes: &[u8]) -> Result<(Vec<Vec<String>>, Vec<u32>), TableError> {
    let text = decoded(bytes)?;
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .delimiter(delimiter(&text))
        .from_reader(text.as_bytes());
    let mut rows = Vec::new();
    let mut lines = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|error| TableError::Read(error.to_string()))?;
        if record.len() > MAX_COLUMNS {
            return Err(TableError::TooManyColumns);
        }
        if record.iter().all(|cell| cell.trim().is_empty()) {
            continue;
        }
        rows.push(record.iter().map(str::to_string).collect());
        lines.push(record.position().map_or(0, |position| position.line() as u32));
        if rows.len() > MAX_ROWS + 1 {
            return Err(TableError::TooManyRows);
        }
    }
    Ok(trimmed(rows, lines))
}

/// Pad every row to the widest one, so columns line up. Blank rows are already left out.
fn trimmed(mut rows: Vec<Vec<String>>, lines: Vec<u32>) -> (Vec<Vec<String>>, Vec<u32>) {
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    for row in &mut rows {
        row.resize(width, String::new());
    }
    (rows, lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(lines: &[&[&str]]) -> Vec<Vec<String>> {
        lines
            .iter()
            .map(|line| line.iter().map(|cell| cell.to_string()).collect())
            .collect()
    }

    fn read_rows(bytes: &[u8]) -> Result<Vec<Vec<String>>, TableError> {
        read_csv(bytes).map(|(rows, _)| rows)
    }

    #[test]
    fn reads_comma_semicolon_and_tab_separated_files() {
        let expected = rows(&[&["Tên", "Effort"], &["Việc, một", "1,5"]]);
        assert_eq!(read_rows("Tên,Effort\n\"Việc, một\",\"1,5\"\n".as_bytes()).unwrap(), expected);
        assert_eq!(read_rows("Tên;Effort\r\n\"Việc, một\";1,5\r\n".as_bytes()).unwrap(), expected);
        assert_eq!(read_rows("Tên\tEffort\n\"Việc, một\"\t1,5\n".as_bytes()).unwrap(), expected);
    }

    #[test]
    fn reads_utf8_with_a_mark_and_utf16_and_refuses_other_encodings() {
        let mut bom = vec![0xEF, 0xBB, 0xBF];
        bom.extend("Tên\nAn\n".as_bytes());
        assert_eq!(read_rows(&bom).unwrap(), rows(&[&["Tên"], &["An"]]));
        let mut utf16 = vec![0xFF, 0xFE];
        for unit in "Tên;Ưu tiên\nAn;900\n".encode_utf16() {
            utf16.extend(unit.to_le_bytes());
        }
        assert_eq!(read_rows(&utf16).unwrap(), rows(&[&["Tên", "Ưu tiên"], &["An", "900"]]));
        // "Tên" in Windows-1258: not UTF-8.
        assert_eq!(read_rows(&[b'T', 0xEA, b'n']), Err(TableError::Encoding));
    }

    #[test]
    fn pads_short_rows_drops_blank_ones_and_keeps_line_numbers() {
        let (found, lines) = read_csv(b"a,b,c\n1\n\n,,\n\"x\ny\",2\n3\n").unwrap();
        assert_eq!(
            found,
            rows(&[&["a", "b", "c"], &["1", "", ""], &["x\ny", "2", ""], &["3", "", ""]])
        );
        // The quoted cell spans lines 5 and 6.
        assert_eq!(lines, [1, 2, 5, 7]);
    }

    #[test]
    fn refuses_tables_too_large() {
        let wide = vec!["x"; MAX_COLUMNS + 1].join(",");
        assert_eq!(read_rows(wide.as_bytes()), Err(TableError::TooManyColumns));
        let long = "x\n".repeat(MAX_ROWS + 2);
        assert_eq!(read_rows(long.as_bytes()), Err(TableError::TooManyRows));
    }

    #[test]
    fn shows_numbers_and_dates_as_a_spreadsheet_does() {
        assert_eq!(number_text(3.0), "3");
        assert_eq!(number_text(1.5), "1.5");
        assert_eq!(cell_text(&Data::Int(-2)), "-2");
        assert_eq!(cell_text(&Data::Empty), "");
        assert_eq!(hours_text(16.0), "16h");
        assert_eq!(hours_text(1.5), "1.5h");
        assert_eq!(iso_text("2026-01-09T13:30:00"), "2026-01-09 13:30");
        assert_eq!(iso_text("2026-01-09T00:00:00"), "2026-01-09");
        assert_eq!(iso_text("2026-01-09"), "2026-01-09");
        assert_eq!(iso_text("PT8H"), "PT8H");
    }

    #[test]
    fn refuses_utf16_cut_in_the_middle_of_a_character() {
        assert_eq!(read_rows(&[0xFF, 0xFE, b'A', 0, b'B']), Err(TableError::Encoding));
    }

    #[test]
    fn reads_a_sheet_of_a_workbook() {
        let dir = std::env::temp_dir().join(format!("sheafly-table-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bang.xlsx");
        let mut workbook = rust_xlsxwriter::Workbook::new();
        workbook.add_worksheet().set_name("Người").unwrap().write(0, 0, "Tên").unwrap();
        let sheet = workbook.add_worksheet().set_name("Việc").unwrap();
        sheet.write(0, 0, "Tên").unwrap();
        sheet.write(0, 1, "Effort").unwrap();
        sheet.write(0, 2, "Hạn").unwrap();
        sheet.write(1, 0, "Thiết kế").unwrap();
        sheet.write(1, 1, 16).unwrap();
        let day = rust_xlsxwriter::ExcelDateTime::from_ymd(2026, 1, 9).unwrap();
        let format = rust_xlsxwriter::Format::new().set_num_format("yyyy-mm-dd");
        sheet.write_datetime_with_format(1, 2, &day, &format).unwrap();
        let blank = rust_xlsxwriter::Format::new().set_bold();
        sheet.write_blank(1, 150, &blank).unwrap();
        let effort = rust_xlsxwriter::Format::new().set_num_format("[h]:mm");
        sheet.write_number_with_format(1, 3, 16.0 / 24.0, &effort).unwrap();
        sheet.write(2, 0, "Họp").unwrap();
        sheet.write(2, 1, 1.5).unwrap();
        let moment = rust_xlsxwriter::ExcelDateTime::from_ymd(2026, 1, 9).unwrap().and_hms(13, 30, 0).unwrap();
        let format = rust_xlsxwriter::Format::new().set_num_format("yyyy-mm-dd hh:mm");
        sheet.write_datetime_with_format(2, 2, &moment, &format).unwrap();
        sheet.write(4, 0, "Sau dòng trống").unwrap();
        workbook.save(&path).unwrap();

        let first = read_table(&path, None).unwrap();
        assert_eq!(first.sheets, ["Người", "Việc"]);
        assert_eq!(first.sheet.as_deref(), Some("Người"));
        let table = read_table(&path, Some("Việc")).unwrap();
        assert_eq!(table.lines, [1, 2, 3, 5]);
        assert_eq!(
            table.rows,
            rows(&[
                &["Tên", "Effort", "Hạn", ""],
                &["Thiết kế", "16", "2026-01-09", "16h"],
                &["Họp", "1.5", "2026-01-09 13:30", ""],
                &["Sau dòng trống", "", "", ""],
            ])
        );
        assert_eq!(read_table(&path, Some("Khác")), Err(TableError::NoSuchSheet));
        std::fs::remove_dir_all(&dir).ok();
    }
}
