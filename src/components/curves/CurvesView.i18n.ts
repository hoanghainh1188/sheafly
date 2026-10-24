import { defineMessages } from "../../i18n";

export const curvesMessages = defineMessages(
  {
    measure: "Chỉ số:",
    measureCost: "Chi phí",
    measureWork: "Giờ công",
    measureProgress: "% Tiến độ",

    period: "Kỳ:",
    periodDay: "Ngày",
    periodWeek: "Tuần",
    periodMonth: "Tháng",

    filter: "Bộ lọc:",
    filterProject: "Toàn dự án",
    filterPerson: "Theo người",
    filterGroup: "Theo vai trò",
    filterSkill: "Theo kỹ năng",
    pickPerson: "Chọn người…",
    pickGroup: "Chọn vai trò…",
    pickSkill: "Chọn kỹ năng…",
    minLevel: "Từ cấp:",
    anyLevel: "(Mọi cấp)",

    baseline: "Baseline:",
    baselineNone: "Không",
    baselineNumber: (n: number) => `Baseline ${n}`,

    mode: "Hiển thị:",
    modeCumulative: "Lũy kế (S-curve)",
    modePeriodic: "Theo kỳ",

    seriesBaseline: "Cơ sở (Baseline)",
    seriesScheduled: "Lịch hiện tại (Scheduled)",
    seriesActual: "Thực tế (Actual)",
    seriesEarned: "Giá trị đạt được (EV)",
    statusDate: "Ngày hiện trạng",
    baselineFinish: "Kết thúc cơ sở",

    loading: "Đang tính đường cong…",
    empty: "Không có dữ liệu cho cấu hình lọc này.",
    tableTitle: "Bảng chi tiết số liệu",
    colPeriod: "Kỳ",
    colBaseline: "Cơ sở",
    colScheduled: "Lịch hiện tại",
    colActual: "Thực tế",
    colEarned: "Đạt được (EV)",
    total: "Tổng cộng",

    chartAria: "Biểu đồ đường cong S và phân bổ theo kỳ",
  },
  {
    measure: "Measure:",
    measureCost: "Cost",
    measureWork: "Work (Hours)",
    measureProgress: "% Progress",

    period: "Period:",
    periodDay: "Days",
    periodWeek: "Weeks",
    periodMonth: "Months",

    filter: "Filter:",
    filterProject: "Entire Project",
    filterPerson: "By Person",
    filterGroup: "By Role",
    filterSkill: "By Skill",
    pickPerson: "Select person…",
    pickGroup: "Select role…",
    pickSkill: "Select skill…",
    minLevel: "Min Level:",
    anyLevel: "(Any level)",

    baseline: "Baseline:",
    baselineNone: "None",
    baselineNumber: (n: number) => `Baseline ${n}`,

    mode: "Mode:",
    modeCumulative: "Cumulative (S-curve)",
    modePeriodic: "Periodic",

    seriesBaseline: "Baseline",
    seriesScheduled: "Current Plan (Scheduled)",
    seriesActual: "Actual",
    seriesEarned: "Earned Value (EV)",
    statusDate: "Status Date",
    baselineFinish: "Baseline Finish",

    loading: "Calculating curves…",
    empty: "No data available for the selected filter.",
    tableTitle: "Detailed Figures Table",
    colPeriod: "Period",
    colBaseline: "Baseline",
    colScheduled: "Scheduled",
    colActual: "Actual",
    colEarned: "Earned (EV)",
    total: "Total",

    chartAria: "S-curve and periodic distribution chart",
  },
);
