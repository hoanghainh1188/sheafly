import { defineMessages } from "../../i18n";

export const estimateGroupMessages = defineMessages(
  {
    legend: "Ước lượng",
    low: "Effort thấp",
    high: "Effort cao",
    note: "Khoảng effort cho dự báo ngày kết thúc; để trống cả hai nếu chắc chắn.",
    needBoth: "Cần cả mức thấp và mức cao (hoặc để trống cả hai)",
    backwards: "Mức thấp lớn hơn mức cao",
    unreadable: "Không đọc được thời lượng",
    outside: (amount: string) => `Effort hiện tại nằm ngoài khoảng (${amount}); dự báo lấy mức gần nhất làm giá trị khả dĩ nhất.`,
  },
  {
    legend: "Estimate",
    low: "Low effort",
    high: "High effort",
    note: "The effort range for the finish forecast; leave both empty when the effort is certain.",
    needBoth: "Enter both a low and a high amount (or leave both empty)",
    backwards: "The low amount is larger than the high one",
    unreadable: "Could not read the amount",
    outside: (amount: string) => `Today's effort lies outside the range (${amount}); the forecast takes the nearest end as the likely value.`,
  },
  {
    legend: "見積り",
    low: "最小工数",
    high: "最大工数",
    note: "完了予測のための工数範囲。工数が確実な場合は両方とも空欄にしてください。",
    needBoth: "最小と最大の両方を入力してください (または両方とも空欄)",
    backwards: "最小工数が最大工数を上回っています",
    unreadable: "工数を読み取れませんでした",
    outside: (amount: string) => `現在の工数が範囲外です (${amount})。予測では近い方の端を最確値として採用します。`,
  },
);
