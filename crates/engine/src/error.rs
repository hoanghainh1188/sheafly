use std::fmt;

use serde::Serialize;

use crate::lang::Lang;
use crate::recurrence::MAX_OCCURRENCES;

/// User-visible engine failures. `message` gives the text in the user's language;
/// `Display` gives the Vietnamese one.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", content = "detail")]
pub enum EngineError {
    DependencyCycle,
    TaskNotFound,
    ResourceNotFound,
    AssignmentNotFound,
    /// The predecessor text the user typed.
    BadPredecessor(String),
    ActualFinishBeforeStart,
    NoStatusDate,
    NoWorkingTime,
    /// A file could not be read. The detail comes from the system or SQLite.
    Store(String),
    /// A file could not be written. The detail comes from the system or SQLite.
    Save(String),
    NewerFormat(u32),
    /// The detail comes from the XML parser.
    Xml(String),
    /// The `version` row of a file is not a number.
    BadFormatVersion(String),
    /// No free name for the temporary file beside the file being saved.
    TempFile,
    /// The autosave slot is empty.
    NoAutosave,
    /// A long read (the finish forecast) was stopped because a newer one replaced it.
    Cancelled,
    /// An edit the engine refuses, with the reason.
    Invalid(Invalid),
}

/// Why an edit was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Invalid {
    NoSuchCalendar,
    EmptyCalendarName,
    WeekNeedsSevenDays,
    BadDate,
    DateRangeTooLong,
    WorkWeekOverlaps,
    NoSuchWorkWeek,
    CalendarBaseLoop,
    InheritedFromBase,
    BadTimeRange,
    OverlappingTimeRanges,
    NotRecurring,
    OccurrenceOutline,
    OvertimeNeedsAssignment,
    DayOfMonth,
    Month,
    WeekOfMonth,
    Weekday,
    TooManyOccurrences,
    OccurrenceCount,
    SeriesEndsBeforeStart,
    NoWeekday,
    NoOccurrences,
    UnitsNotPositive,
    BadDuration,
    SplitOutsideWork,
    SplitSummary,
    SplitNoDuration,
    SplitGapNotPositive,
    NotSplit,
    NoSuchSplit,
    FirstSegment,
    NothingToPaste,
    FieldName,
    FieldNameTaken,
    NoSuchField,
    TooManyTextFields,
    TooManyNumberFields,
    ChoiceOptions,
    FieldValue,
    DuplicateSortKey,
    TooManySortKeys,
    AvailabilityRange,
    AvailabilityUnits,
    AvailabilityOverlaps,
    TooManyAvailabilityPeriods,
    SkillName,
    SkillNameTaken,
    NoSuchSkill,
    TooManySkills,
    SkillLevelName,
    SkillLevelNameTaken,
    NoSuchSkillLevel,
    TooManySkillLevels,
    LevelWithoutSkill,
    ResourceGroup,
    FocusFactor,
    TooManyRules,
    TooManyConditions,
    RuleUnits,
    EmptyCondition,
    DuplicateCondition,
    NoAssignments,
    InterruptsLeaf,
    WorkRangeLeaf,
    BadWorkRange,
    ForecastFieldKind,
    NothingToImport,
    ImportHasErrors,
    TooManyImportRows,
    ImportName,
    ImportNameRepeated,
    ImportRate,
    ImportLevel,
    ImportNegative,
    ImportWorkAndDuration,
    ImportGroupWork,
    ImportPriority,
    ImportUnknownPerson,
    ImportLinkRow,
    ImportLinkLoop,
    EmptyPlan,
    EmptyProposal,
    NotInProposal,
    EmailInvalid,
    EmailTaken,
    SourceSite,
    SourceProjectKey,
    SourceFilter,
    SourceHoursPerPoint,
    SourceStatus,
    SourceStatusRepeated,
    NothingToPull,
    PullHasErrors,
    TooManyPullRows,
    PullKey,
    PullKeyRepeated,
    PullName,
    PullNegative,
    PullUnknownAssignee,
    PullParent,
    PullLink,
    PushDate,
    NothingToRecord,
    PullHasConflicts,
    SyncPolicy,
    SyncSetting,
    TooManyPayments,
    PaymentName,
    PaymentAmount,
    PaymentLag,
    RetentionPercent,
    NoSuchPayment,
    PaymentRepeated,
    TooManyReceipts,
}

impl EngineError {
    /// The message to show the user, in `lang`.
    pub fn message(&self, lang: Lang) -> String {
        match (self, lang) {
            (Self::DependencyCycle, Lang::Vi) => "Phụ thuộc tạo thành vòng lặp".into(),
            (Self::DependencyCycle, Lang::En) => "The links between tasks form a loop".into(),
            (Self::TaskNotFound, Lang::Vi) => "Không tìm thấy công việc".into(),
            (Self::TaskNotFound, Lang::En) => "Task not found".into(),
            (Self::ResourceNotFound, Lang::Vi) => "Không tìm thấy nguồn lực".into(),
            (Self::ResourceNotFound, Lang::En) => "Resource not found".into(),
            (Self::AssignmentNotFound, Lang::Vi) => "Không tìm thấy phân công".into(),
            (Self::AssignmentNotFound, Lang::En) => "Assignment not found".into(),
            (Self::BadPredecessor(text), Lang::Vi) => format!("Tiền nhiệm không hợp lệ: {text}"),
            (Self::BadPredecessor(text), Lang::En) => format!("Invalid predecessor: {text}"),
            (Self::ActualFinishBeforeStart, Lang::Vi) => {
                "Ngày kết thúc thực tế phải sau ngày bắt đầu thực tế".into()
            }
            (Self::ActualFinishBeforeStart, Lang::En) => {
                "The actual finish must be after the actual start".into()
            }
            (Self::NoStatusDate, Lang::Vi) => "Chưa đặt ngày trạng thái".into(),
            (Self::NoStatusDate, Lang::En) => "The status date is not set".into(),
            (Self::NoWorkingTime, Lang::Vi) => "Lịch không có thời gian làm việc".into(),
            (Self::NoWorkingTime, Lang::En) => "The calendar has no working time".into(),
            (Self::Store(detail), Lang::Vi) => format!("Không đọc được tệp: {detail}"),
            (Self::Store(detail), Lang::En) => format!("Could not read the file: {detail}"),
            (Self::Save(detail), Lang::Vi) => format!("Không lưu được tệp: {detail}"),
            (Self::Save(detail), Lang::En) => format!("Could not save the file: {detail}"),
            (Self::NewerFormat(version), Lang::Vi) => format!(
                "Tệp được tạo bởi phiên bản mới hơn của Sheafly (định dạng {version}). \
                 Hãy cập nhật ứng dụng."
            ),
            (Self::NewerFormat(version), Lang::En) => format!(
                "This file was created by a newer version of Sheafly (format {version}). \
                 Please update the app."
            ),
            (Self::Xml(detail), Lang::Vi) => format!("XML MSPDI không hợp lệ: {detail}"),
            (Self::Xml(detail), Lang::En) => format!("Invalid MSPDI XML: {detail}"),
            (Self::BadFormatVersion(stored), Lang::Vi) => {
                format!("Không đọc được tệp: phiên bản định dạng không hợp lệ: {stored}")
            }
            (Self::BadFormatVersion(stored), Lang::En) => {
                format!("Could not read the file: invalid format version: {stored}")
            }
            (Self::TempFile, Lang::Vi) => "Không lưu được tệp: không tạo được tệp tạm".into(),
            (Self::TempFile, Lang::En) => {
                "Could not save the file: could not create a temporary file".into()
            }
            (Self::NoAutosave, Lang::Vi) => "Không đọc được tệp: không có bản tự lưu".into(),
            (Self::NoAutosave, Lang::En) => "Could not read the file: there is no autosave".into(),
            (Self::Cancelled, Lang::Vi) => "Đã dừng tính vì dự án vừa thay đổi".into(),
            (Self::Cancelled, Lang::En) => "Stopped because the project changed".into(),
            (Self::Invalid(reason), _) => reason.message(lang),
        }
    }
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message(Lang::Vi))
    }
}

impl std::error::Error for EngineError {}

impl From<Invalid> for EngineError {
    fn from(reason: Invalid) -> Self {
        Self::Invalid(reason)
    }
}

impl Invalid {
    pub fn message(self, lang: Lang) -> String {
        let (vi, en) = self.texts();
        lang.pick(vi, en)
    }

    fn texts(self) -> (String, String) {
        let fixed = |vi: &str, en: &str| (vi.to_string(), en.to_string());
        match self {
            Self::NoSuchCalendar => fixed("Không có lịch này", "That calendar does not exist"),
            Self::EmptyCalendarName => fixed("Tên lịch trống", "The calendar name is empty"),
            Self::WeekNeedsSevenDays => {
                fixed("Tuần làm việc phải có 7 ngày", "A work week must have 7 days")
            }
            Self::BadDate => fixed("Ngày không hợp lệ", "Invalid date"),
            Self::WorkWeekOverlaps => fixed(
                "Tuần làm việc trùng ngày với một tuần làm việc khác",
                "That work week overlaps another work week",
            ),
            Self::CalendarBaseLoop => fixed(
                "Lịch không thể dựa trên chính nó hoặc trên lịch dựa trên nó",
                "A calendar cannot be based on itself or on a calendar based on it",
            ),
            Self::InheritedFromBase => fixed(
                "Mục này đến từ lịch gốc: hãy sửa ở lịch gốc, hoặc đặt giờ khác cho những ngày đó",
                "This comes from the base calendar: change it there, or set other hours for those days",
            ),
            Self::NoSuchWorkWeek => fixed(
                "Không có tuần làm việc này",
                "That work week does not exist",
            ),
            Self::DateRangeTooLong => fixed("Khoảng ngày quá dài", "The date range is too long"),
            Self::BadTimeRange => fixed("Khoảng giờ không hợp lệ", "Invalid working time range"),
            Self::OverlappingTimeRanges => fixed(
                "Các khoảng giờ bị chồng nhau",
                "The working time ranges overlap",
            ),
            Self::NotRecurring => fixed(
                "Công việc này không phải chuỗi lặp",
                "This task is not a recurring task",
            ),
            Self::OccurrenceOutline => fixed(
                "Không đổi cấp một lần lặp; hãy sửa cả chuỗi",
                "You cannot indent or outdent a single occurrence; change the whole \
                 recurring task",
            ),
            Self::OvertimeNeedsAssignment => fixed(
                "Hãy gán nguồn lực trước khi nhập giờ làm thêm",
                "Assign a resource before entering overtime work",
            ),
            Self::DayOfMonth => fixed(
                "Ngày trong tháng phải từ 1 đến 31",
                "The day of the month must be from 1 to 31",
            ),
            Self::Month => fixed("Tháng phải từ 1 đến 12", "The month must be from 1 to 12"),
            Self::WeekOfMonth => fixed(
                "Tuần trong tháng phải là thứ nhất đến thứ tư hoặc cuối cùng",
                "The week of the month must be the first, second, third, fourth, or last",
            ),
            Self::Weekday => fixed("Thứ trong tuần không hợp lệ", "Invalid day of the week"),
            Self::TooManyOccurrences => (
                format!("Chuỗi lặp vượt quá {MAX_OCCURRENCES} lần"),
                format!("A recurring task cannot have more than {MAX_OCCURRENCES} occurrences"),
            ),
            Self::OccurrenceCount => (
                format!("Số lần lặp phải từ 1 đến {MAX_OCCURRENCES}"),
                format!("The number of occurrences must be from 1 to {MAX_OCCURRENCES}"),
            ),
            Self::SeriesEndsBeforeStart => fixed(
                "Ngày kết thúc chuỗi phải sau ngày bắt đầu",
                "The recurrence must end after it starts",
            ),
            Self::NoWeekday => fixed(
                "Hãy chọn ít nhất một thứ trong tuần",
                "Select at least one day of the week",
            ),
            Self::NoOccurrences => fixed(
                "Chuỗi lặp không có lần nào",
                "The recurring task has no occurrences",
            ),
            Self::UnitsNotPositive => fixed(
                "Đơn vị nguồn lực phải lớn hơn 0",
                "Assignment units must be greater than 0",
            ),
            Self::BadDuration => fixed("Thời lượng không hợp lệ", "Invalid duration"),
            Self::SplitOutsideWork => fixed(
                "Điểm tách phải nằm trong thời gian làm việc của công việc",
                "The split must fall within the task's working time",
            ),
            Self::SplitSummary => fixed(
                "Không tách được công việc tóm tắt",
                "A summary task cannot be split",
            ),
            Self::SplitNoDuration => fixed(
                "Công việc không có thời lượng nên không tách được",
                "A task with no duration cannot be split",
            ),
            Self::SplitGapNotPositive => fixed(
                "Khoảng ngắt phải lớn hơn 0",
                "The split length must be greater than 0",
            ),
            Self::NotSplit => fixed("Công việc này chưa bị tách", "This task is not split"),
            Self::NoSuchSplit => fixed("Không có khoảng ngắt này", "That split does not exist"),
            Self::FirstSegment => fixed(
                "Đoạn đầu dời cùng cả công việc; hãy kéo cả thanh",
                "The first segment moves with the whole task; drag the bar instead",
            ),
            Self::NothingToPaste => fixed("Không có gì để dán", "There is nothing to paste"),
            Self::FieldName => fixed(
                "Tên trường phải có từ 1 đến 50 ký tự",
                "A field name must have 1 to 50 characters",
            ),
            Self::FieldNameTaken => fixed(
                "Đã có trường cùng tên",
                "There is already a field with that name",
            ),
            Self::NoSuchField => fixed("Không có trường này", "That field does not exist"),
            Self::TooManyTextFields => fixed(
                "Dự án đã có 30 trường chữ hoặc danh sách, mức tối đa Microsoft Project giữ được",
                "The project already has 30 text or list fields, the most Microsoft Project can hold",
            ),
            Self::TooManyNumberFields => fixed(
                "Dự án đã có 20 trường số, mức tối đa Microsoft Project giữ được",
                "The project already has 20 number fields, the most Microsoft Project can hold",
            ),
            Self::ChoiceOptions => fixed(
                "Danh sách cần ít nhất một lựa chọn, không trống, không trùng, tối đa 255 ký tự",
                "A list needs at least one choice; none empty, repeated, or over 255 characters",
            ),
            Self::FieldValue => fixed(
                "Giá trị không hợp với trường này",
                "That value does not fit this field",
            ),
            Self::DuplicateSortKey => fixed(
                "Mỗi tiêu chí chỉ có một lần trong thứ tự cân bằng",
                "Each criterion can appear only once in the leveling order",
            ),
            Self::TooManySortKeys => fixed(
                "Thứ tự cân bằng có tối đa 8 tiêu chí",
                "The leveling order can have at most 8 criteria",
            ),
            Self::AvailabilityRange => fixed(
                "Khoảng khả dụng phải kết thúc vào hoặc sau ngày bắt đầu",
                "An availability period must end on or after its start",
            ),
            Self::AvailabilityUnits => fixed(
                "Đơn vị khả dụng phải là một số không âm",
                "Available units must be a number of zero or more",
            ),
            Self::AvailabilityOverlaps => fixed(
                "Các khoảng khả dụng của một nguồn lực không được trùng ngày",
                "The availability periods of a resource cannot share days",
            ),
            Self::TooManyAvailabilityPeriods => fixed(
                "Một nguồn lực có tối đa 500 khoảng khả dụng",
                "A resource can have at most 500 availability periods",
            ),
            Self::SkillName => fixed(
                "Tên kỹ năng phải có từ 1 đến 50 ký tự, không có dấu phẩy hay dấu hai chấm",
                "A skill name must have 1 to 50 characters and no comma or colon",
            ),
            Self::SkillNameTaken => fixed(
                "Đã có kỹ năng cùng tên",
                "There is already a skill with that name",
            ),
            Self::NoSuchSkill => fixed("Không có kỹ năng này", "That skill does not exist"),
            Self::TooManySkills => fixed(
                "Dự án có tối đa 200 kỹ năng",
                "A project can have at most 200 skills",
            ),
            Self::SkillLevelName => fixed(
                "Tên cấp độ phải có từ 1 đến 30 ký tự, không có dấu phẩy hay dấu hai chấm",
                "A level name must have 1 to 30 characters and no comma or colon",
            ),
            Self::SkillLevelNameTaken => fixed(
                "Hai cấp độ không được trùng tên",
                "Two levels cannot have the same name",
            ),
            Self::NoSuchSkillLevel => fixed("Không có cấp độ này", "That level does not exist"),
            Self::TooManySkillLevels => fixed(
                "Thang cấp độ có tối đa 20 bậc",
                "A level scale can have at most 20 levels",
            ),
            Self::LevelWithoutSkill => fixed(
                "Chỉ đặt cấp độ cho kỹ năng mà nguồn lực có",
                "A level can only be set for a skill the resource has",
            ),
            Self::ResourceGroup => fixed(
                "Vai trò có tối đa 100 ký tự",
                "A role can have at most 100 characters",
            ),
            Self::FocusFactor => fixed(
                "Năng suất hữu ích phải từ 50% đến 100%",
                "Useful productivity must be from 50% to 100%",
            ),
            Self::TooManyRules => fixed(
                "Dự án có tối đa 200 quy tắc giao việc",
                "A project can have at most 200 assignment rules",
            ),
            Self::TooManyConditions => fixed(
                "Một quy tắc có tối đa 8 điều kiện",
                "A rule can have at most 8 conditions",
            ),
            Self::RuleUnits => fixed(
                "Đơn vị của quy tắc phải lớn hơn 0 và không quá 1.000",
                "A rule's units must be above 0 and at most 1,000",
            ),
            Self::EmptyCondition => fixed(
                "Mỗi điều kiện cần ít nhất một giá trị",
                "Each condition needs at least one value",
            ),
            Self::DuplicateCondition => fixed(
                "Một quy tắc chỉ có một điều kiện cho mỗi trường",
                "A rule can have only one condition per field",
            ),
            Self::NoAssignments => fixed("Không có việc nào để giao", "There is nothing to assign"),
            Self::InterruptsLeaf => fixed(
                "Chỉ công việc có thời lượng mới chen ngang được, không phải tóm tắt hay mốc",
                "Only a task with a duration can interrupt, not a summary or a milestone",
            ),
            Self::WorkRangeLeaf => fixed(
                "Chỉ công việc có thời lượng mới có khoảng effort, không phải tóm tắt hay mốc",
                "Only a task with a duration can have an effort range, not a summary or a milestone",
            ),
            Self::BadWorkRange => fixed(
                "Khoảng effort cần cả mức thấp và mức cao, không âm, mức thấp không lớn hơn mức cao",
                "An effort range needs both a low and a high amount, not negative, the low one no larger",
            ),
            Self::ForecastFieldKind => fixed(
                "Chỉ trường văn bản hoặc danh sách chọn mới nhóm được công việc cho dự báo",
                "Only a text or choice field can group tasks for the forecast",
            ),
            Self::NothingToImport => fixed(
                "Không có dòng nào để nhập",
                "There are no rows to import",
            ),
            Self::ImportHasErrors => fixed(
                "Bảng nhập còn dòng lỗi",
                "Some rows of the import have problems",
            ),
            Self::TooManyImportRows => fixed(
                "Một lần nhập có tối đa 20.000 dòng",
                "An import can have at most 20,000 rows",
            ),
            Self::ImportName => fixed(
                "Tên trống",
                "The name is empty",
            ),
            Self::ImportNameRepeated => fixed(
                "Tên này đã có ở dòng trên",
                "This name is already on a row above",
            ),
            Self::ImportRate => fixed(
                "Đơn giá không được âm",
                "The rate cannot be negative",
            ),
            Self::ImportLevel => fixed(
                "Cấp chỉ được sâu hơn dòng trên một bậc",
                "A level can be at most one deeper than the row above",
            ),
            Self::ImportNegative => fixed(
                "Effort và thời lượng không được âm",
                "Effort and duration cannot be negative",
            ),
            Self::ImportWorkAndDuration => fixed(
                "Chỉ ghi effort hoặc thời lượng, không cả hai",
                "Give either the effort or the duration, not both",
            ),
            Self::ImportGroupWork => fixed(
                "Dòng nhóm (có dòng con) không có effort, thời lượng hay người",
                "A group row (one with rows under it) takes no effort, duration or people",
            ),
            Self::ImportPriority => fixed(
                "Ưu tiên từ 0 đến 1.000",
                "The priority must be from 0 to 1,000",
            ),
            Self::ImportUnknownPerson => fixed(
                "Dự án không có người tên này",
                "The project has no one by this name",
            ),
            Self::ImportLinkRow => fixed(
                "Chờ một dòng không có trong bảng hoặc chính dòng này",
                "Waits for a row that is not in the table, or for itself",
            ),
            Self::EmptyPlan => fixed(
                "Kế hoạch không có gì để áp dụng",
                "The plan has nothing to apply",
            ),
            Self::EmptyProposal => fixed(
                "Đề xuất không có lệnh nào",
                "The proposal has no command",
            ),
            Self::NotInProposal => fixed(
                "Đề xuất chứa một lệnh không được phép",
                "The proposal holds a command that is not allowed in one",
            ),
            Self::ImportLinkLoop => fixed(
                "Các dòng chờ nhau thành vòng",
                "These rows wait for each other in a loop",
            ),
            Self::EmailInvalid => fixed(
                "Địa chỉ email không hợp lệ",
                "The email address is not valid",
            ),
            Self::EmailTaken => fixed(
                "Email này đã thuộc về người khác trong dự án",
                "Another person in the project already has this email",
            ),
            Self::SourceSite => fixed(
                "Tên site không hợp lệ (chỉ tên máy chủ, ví dụ team.backlog.com)",
                "The site name is not valid (a host name only, such as team.backlog.com)",
            ),
            Self::SourceProjectKey => fixed(
                "Khóa dự án ở nguồn trống hoặc quá dài",
                "The project key at the source is empty or too long",
            ),
            Self::SourceFilter => fixed(
                "Bộ lọc tối đa 500 ký tự",
                "The filter can have at most 500 characters",
            ),
            Self::SourceHoursPerPoint => fixed(
                "Số giờ mỗi điểm phải lớn hơn 0 và không quá 1.000",
                "Hours per point must be above 0 and at most 1,000",
            ),
            Self::SourceStatus => fixed(
                "Tên trạng thái trống hoặc quá dài, hoặc có quá nhiều trạng thái",
                "A status name is empty or too long, or there are too many statuses",
            ),
            Self::SourceStatusRepeated => fixed(
                "Trạng thái này đã được ánh xạ",
                "This status is already mapped",
            ),
            Self::NothingToPull => fixed(
                "Không có gì thay đổi ở nguồn",
                "Nothing has changed at the source",
            ),
            Self::PullHasErrors => fixed(
                "Một số công việc kéo về còn lỗi",
                "Some of the pulled issues have problems",
            ),
            Self::TooManyPullRows => fixed(
                "Một lần kéo có tối đa 20.000 công việc",
                "A pull can have at most 20,000 issues",
            ),
            Self::PullKey => fixed(
                "Khóa công việc ở nguồn trống",
                "The issue key at the source is empty",
            ),
            Self::PullKeyRepeated => fixed(
                "Khóa này đã có ở dòng trên",
                "This key is already on a row above",
            ),
            Self::PullName => fixed(
                "Tên công việc trống",
                "The issue has no title",
            ),
            Self::PullNegative => fixed(
                "Effort không được âm",
                "The estimate cannot be negative",
            ),
            Self::PullUnknownAssignee => fixed(
                "Dự án không có người nào dùng email này",
                "No one in the project has this email",
            ),
            Self::PullParent => fixed(
                "Công việc cha không có trong lần kéo, hoặc nằm sau dòng này",
                "The parent issue is not in the pull, or comes after this row",
            ),
            Self::PullLink => fixed(
                "Chờ một công việc không có trong lần kéo, hoặc chính nó",
                "Waits for an issue that is not in the pull, or for itself",
            ),
            Self::PushDate => fixed(
                "Ngày ghi về nguồn không hợp lệ",
                "The day written to the source is not valid",
            ),
            Self::NothingToRecord => fixed(
                "Không có công việc liên kết nào khớp để ghi nhớ",
                "No linked task matches, so there is nothing to record",
            ),
            Self::PullHasConflicts => fixed(
                "Một số trường đã đổi ở cả hai phía; hãy chọn giữ bên nào",
                "Some fields changed on both sides; choose which to keep",
            ),
            Self::SyncPolicy => fixed(
                "Chính sách đồng bộ không hợp lệ (khóa trường sai, lặp, quá dài hoặc quá nhiều)",
                "A sync policy is not valid (a bad, repeated or too long field, or too many)",
            ),
            Self::SyncSetting => fixed(
                "Phần trăm đang làm phải từ 1 đến 99 và giờ phải từ 0 đến 23",
                "The in-progress percent must be 1 to 99 and the hours 0 to 23",
            ),
            Self::TooManyPayments => fixed(
                "Một dự án có tối đa 500 khoản thu",
                "A project can have at most 500 payments",
            ),
            Self::PaymentName => fixed(
                "Tên khoản thu không được trống và dài tối đa 200 ký tự",
                "A payment needs a name of at most 200 characters",
            ),
            Self::PaymentAmount => fixed(
                "Số tiền phải là số không âm (phần trăm từ 0 đến 100)",
                "An amount must be a number not below 0 (a percent 0 to 100)",
            ),
            Self::PaymentLag => fixed(
                "Hạn thanh toán phải từ 0 đến 365 ngày",
                "Payment terms must be 0 to 365 days",
            ),
            Self::RetentionPercent => fixed(
                "Phần giữ lại phải từ 0 đến 50 %",
                "The retention must be 0 to 50 percent",
            ),
            Self::NoSuchPayment => fixed("Không có khoản thu này", "That payment does not exist"),
            Self::PaymentRepeated => fixed(
                "Một khoản thu xuất hiện hai lần trong danh sách",
                "A payment appears twice in the list",
            ),
            Self::TooManyReceipts => fixed(
                "Một khoản thu có tối đa 500 lần ghi nhận tiền",
                "A payment can have at most 500 receipts",
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_stays_vietnamese() {
        assert_eq!(EngineError::TaskNotFound.to_string(), "Không tìm thấy công việc");
        assert_eq!(
            EngineError::Invalid(Invalid::NoSuchCalendar).to_string(),
            "Không có lịch này"
        );
    }

    #[test]
    fn every_reason_has_both_languages() {
        use Invalid::*;
        let all = [
            NoSuchCalendar,
            EmptyCalendarName,
            WeekNeedsSevenDays,
            BadDate,
            DateRangeTooLong,
            WorkWeekOverlaps,
            NoSuchWorkWeek,
            CalendarBaseLoop,
            InheritedFromBase,
            BadTimeRange,
            OverlappingTimeRanges,
            NotRecurring,
            OccurrenceOutline,
            OvertimeNeedsAssignment,
            DayOfMonth,
            Month,
            WeekOfMonth,
            Weekday,
            TooManyOccurrences,
            OccurrenceCount,
            SeriesEndsBeforeStart,
            NoWeekday,
            NoOccurrences,
            UnitsNotPositive,
            BadDuration,
            SplitOutsideWork,
            SplitSummary,
            SplitNoDuration,
            SplitGapNotPositive,
            NotSplit,
            NoSuchSplit,
            FirstSegment,
            NothingToPaste,
            FieldName,
            FieldNameTaken,
            NoSuchField,
            TooManyTextFields,
            TooManyNumberFields,
            ChoiceOptions,
            FieldValue,
            DuplicateSortKey,
            TooManySortKeys,
            AvailabilityRange,
            AvailabilityUnits,
            AvailabilityOverlaps,
            TooManyAvailabilityPeriods,
            SkillName,
            SkillNameTaken,
            NoSuchSkill,
            TooManySkills,
            SkillLevelName,
            SkillLevelNameTaken,
            NoSuchSkillLevel,
            TooManySkillLevels,
            LevelWithoutSkill,
            ResourceGroup,
            FocusFactor,
            TooManyRules,
            TooManyConditions,
            RuleUnits,
            EmptyCondition,
            DuplicateCondition,
            NoAssignments,
            InterruptsLeaf,
            WorkRangeLeaf,
            BadWorkRange,
            ForecastFieldKind,
            NothingToImport,
            ImportHasErrors,
            TooManyImportRows,
            ImportName,
            ImportNameRepeated,
            ImportRate,
            ImportLevel,
            ImportNegative,
            ImportWorkAndDuration,
            ImportGroupWork,
            ImportPriority,
            ImportUnknownPerson,
            ImportLinkRow,
            ImportLinkLoop,
            EmptyPlan,
            EmptyProposal,
            NotInProposal,
            EmailInvalid,
            EmailTaken,
            SourceSite,
            SourceProjectKey,
            SourceFilter,
            SourceHoursPerPoint,
            SourceStatus,
            SourceStatusRepeated,
            NothingToPull,
            PullHasErrors,
            TooManyPullRows,
            PullKey,
            PullKeyRepeated,
            PullName,
            PullNegative,
            PullUnknownAssignee,
            PullParent,
            PullLink,
            PushDate,
            NothingToRecord,
            PullHasConflicts,
            SyncPolicy,
            SyncSetting,
            TooManyPayments,
            PaymentName,
            PaymentAmount,
            PaymentLag,
            RetentionPercent,
            NoSuchPayment,
            PaymentRepeated,
            TooManyReceipts,
        ];
        for reason in all {
            let (vi, en) = (reason.message(Lang::Vi), reason.message(Lang::En));
            assert!(!vi.is_empty() && !en.is_empty(), "{reason:?}");
            assert_ne!(vi, en, "{reason:?}");
            assert!(en.is_ascii(), "{reason:?}: {en}");
        }
    }
}
