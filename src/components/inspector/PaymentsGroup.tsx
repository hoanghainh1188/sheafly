import { Banknote } from "lucide-react";

import { useMessages } from "../../i18n";
import type { Project, Task } from "../../types";
import { Button } from "../Button";
import { followsTask } from "../payments/paymentText";
import { paymentMessages } from "../payments/payments.i18n";

interface Props {
  project: Project;
  task: Task;
  onOpen: () => void;
}

/**
 * Thanh toán: the payments that follow this task, and the way to the payment schedule.
 * Shown for milestones and groups, which contracts pay by, and for any task a payment follows.
 */
export function PaymentsGroup({ project, task, onOpen }: Props) {
  const m = useMessages(paymentMessages);
  const payments = project.payments.filter((payment) => followsTask(payment.anchor, task.uid));
  const release = project.cash_flow.retention;
  const released = release != null && followsTask(release.release, task.uid);
  if (!task.milestone && !task.summary && payments.length === 0 && !released) return null;
  const when = (type: string) => (type === "start" ? m.start : m.finish);
  return (
    <fieldset className="inspector-group payments-group">
      <legend>{m.group}</legend>
      {payments.length === 0 && !released ? (
        <p className="meta">{m.groupNone}</p>
      ) : (
        <ul className="payments-group-list">
          {payments.map((payment) => (
            <li key={payment.id}>{m.groupItem(payment.name.trim() || "—", when(payment.anchor.type), payment.lag_days)}</li>
          ))}
          {released && <li>{m.groupRelease(when(release.release.type))}</li>}
        </ul>
      )}
      <div>
        <Button Icon={Banknote} onClick={onOpen}>{m.open}</Button>
      </div>
    </fieldset>
  );
}
