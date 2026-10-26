import { useMessages } from "../../i18n";
import type { PaymentAnchor, Project } from "../../types";
import { PickerField } from "../DateTimeField";
import { anchorTasks } from "./paymentText";
import { paymentMessages } from "./payments.i18n";

interface Props {
  project: Project;
  anchor: PaymentAnchor;
  /** Names the controls, as "Gắn với" or "Giải ngân khi" plus the payment. */
  label: string;
  onChange: (anchor: PaymentAnchor) => void;
}

const DATE = "date";

/**
 * What a payment is invoiced at: a task's finish or start, or a fixed date. Milestones and
 * groups come first in the list, since contracts pay by milestone.
 */
export function AnchorPicker({ project, anchor, label, onChange }: Props) {
  const m = useMessages(paymentMessages);
  const tasks = anchorTasks(project);
  const split = tasks.findIndex((task) => !task.milestone && !task.summary);
  const first = split < 0 ? tasks : tasks.slice(0, split);
  const other = split < 0 ? [] : tasks.slice(split);
  const option = (uid: number, id: number, name: string) => (
    <option key={uid} value={uid}>
      {m.taskOption(id, name.trim() || "—")}
    </option>
  );
  const pick = (value: string) => {
    if (value === DATE) {
      // A fixed date starts where the task was, so switching does not move the money.
      const task = anchor.type === "date" ? undefined : project.tasks.find((item) => item.uid === anchor.task);
      const at = task ? (anchor.type === "start" ? task.start : task.finish) : project.start;
      onChange({ type: "date", at: Math.floor(at / 86_400_000) * 86_400_000 });
      return;
    }
    const uid = Number(value);
    onChange({ type: anchor.type === "start" ? "start" : "finish", task: uid });
  };
  return (
    <span className="payment-anchor">
      <select aria-label={label} value={anchor.type === "date" ? DATE : String(anchor.task)} onChange={(event) => pick(event.target.value)}>
        {first.length > 0 && <optgroup label={m.groupFirst}>{first.map((task) => option(task.uid, task.id, task.name))}</optgroup>}
        {other.length > 0 && <optgroup label={m.groupOther}>{other.map((task) => option(task.uid, task.id, task.name))}</optgroup>}
        <option value={DATE}>{m.anchorDate}</option>
      </select>
      {anchor.type === "date" ? (
        <PickerField
          mode="date"
          value={anchor.at}
          ariaLabel={`${label}: ${m.date}`}
          onCommit={(at) => at != null && onChange({ type: "date", at })}
        />
      ) : (
        <select
          aria-label={`${label}: ${m.anchorWhen}`}
          value={anchor.type}
          onChange={(event) => onChange({ type: event.target.value === "start" ? "start" : "finish", task: anchor.task })}
        >
          <option value="finish">{m.finish}</option>
          <option value="start">{m.start}</option>
        </select>
      )}
    </span>
  );
}
