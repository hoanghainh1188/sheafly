import { defineMessages } from "../i18n";

export const forecastTabMessages = defineMessages(
  {
    intro:
      "Dự báo ngày kết thúc lấy mẫu effort của mỗi việc trong khoảng Effort thấp–cao của nó. Các việc thường trễ cùng nhau (cùng một mảng, cùng một đội ước lượng lạc quan), nên dự báo cho chúng tương quan với nhau.",
    field: "Tương quan theo",
    noField: "Không theo trường nào",
    fieldNote: "Việc cùng giá trị của trường này (văn bản hoặc danh sách chọn) trễ hoặc sớm cùng nhau.",
    group: "Mức trong nhóm",
    groupNeedsField: "Chọn một trường ở trên để dùng mức trong nhóm.",
    common: "Mức chung",
    commonNote: "Phần mọi việc cùng chịu, như cả đội cùng ước lượng lạc quan.",
    none: "Không",
    low: "Thấp",
    medium: "Vừa",
    high: "Cao",
  },
  {
    intro:
      "The finish forecast samples each task's effort within its low–high effort range. Tasks often slip together (the same area, the same team estimating optimistically), so the forecast correlates them.",
    field: "Correlate by",
    noField: "No field",
    fieldNote: "Tasks with the same value of this field (text or choice) run late or early together.",
    group: "Within a group",
    groupNeedsField: "Choose a field above to use the strength within a group.",
    common: "Across all tasks",
    commonNote: "The part every task shares, such as a whole team estimating optimistically.",
    none: "None",
    low: "Low",
    medium: "Medium",
    high: "High",
  },
  {
    intro:
      "完了予測は、各タスクの最小〜最大工数の範囲内で工数をサンプリングして計算します。同じ業務領域や同一チームの見積りは連動して遅延する傾向があるため、予測モデルではそれらの相関関係を考慮します。",
    field: "相関基準フィールド",
    noField: "フィールド指定なし",
    fieldNote: "このフィールド (テキストまたはリスト) で同一の値を持つタスク同士が連動して遅延または早期完了します。",
    group: "グループ内相関度",
    groupNeedsField: "グループ内相関度を設定するには、上のフィールドを選択してください。",
    common: "全体相関度",
    commonNote: "プロジェクト全体で共有される不確実性の度合い (例: チーム全体の見積りの甘さなど)。",
    none: "なし",
    low: "低",
    medium: "中",
    high: "高",
  },
);
