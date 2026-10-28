import { defineMessages } from "./i18n";

export const assignmentUnitsMessages = defineMessages(
  {
    perDay: (hours: string, _one: boolean) => `${hours} giờ/ngày`,
  },
  {
    perDay: (hours: string, one: boolean) => `${hours} ${one ? "hr" : "hrs"}/day`,
  },
  {
    perDay: (hours: string, _one: boolean) => `${hours} 時間/日`,
  },
);
