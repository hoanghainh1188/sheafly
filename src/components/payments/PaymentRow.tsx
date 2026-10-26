import { Trash2, Wallet } from "lucide-react";

import { formatDateOnly } from "../../format";
import { useMessages } from "../../i18n";
import type { Payment, PaymentDates, PaymentInput, Project } from "../../types";
import { Button } from "../Button";
import { MoneyInput } from "../MoneyInput";
import { AnchorPicker } from "./AnchorPicker";
import { ReadField } from "./ReadField";
import { formatPercent, parseLagDays, parsePercent } from "./paymentText";
import { paymentMessages } from "./payments.i18n";

interface Props {
  project: Project;
  payment: Payment;
  /** As scheduled now, from the engine; undefined while it answers, null for a task that is gone. */
  dates: PaymentDates | null | undefined;
  onChange: (input: Partial<PaymentInput>) => void;
  onRemove: () => void;
  onReceipts: () => void;
}

/** One payment of the schedule: its terms, the dates the engine gives them, and what came in. */
export function PaymentRow({ project, payment, dates, onChange, onRemove, onReceipts }: Props) {
  const m = useMessages(paymentMessages);
  const name = payment.name.trim() || "—";
  const percent = "percent" in payment.amount;
  const dateText = (at: number | undefined) => (at == null ? m.noDate : formatDateOnly(at));
  return (
    <tr className={payment.detached ? "payment-row detached" : "payment-row"}>
      <td>
        <ReadField
          label={m.name}
          shown={payment.name}
          read={(text) => (text.trim() ? text.trim() : null)}
          refusal={m.nameRequired}
          onCommit={(value) => onChange({ name: String(value) })}
        />
        {payment.detached && <span className="payment-note">{m.detached}</span>}
      </td>
      <td>
        <span className="payment-amount">
          <select
            aria-label={`${m.amountKind}: ${name}`}
            value={percent ? "percent" : "fixed"}
            onChange={(event) => onChange({ amount: event.target.value === "percent" ? { percent: 0 } : { fixed: 0 } })}
          >
            <option value="fixed">{m.fixed}</option>
            <option value="percent">{m.percent}</option>
          </select>
          {"percent" in payment.amount ? (
            <ReadField
              label={`${m.amount}: ${name}`}
              className="narrow"
              shown={formatPercent(payment.amount.percent)}
              read={(text) => parsePercent(text)}
              refusal={m.percentUnreadable}
              onCommit={(value) => onChange({ amount: { percent: Number(value) } })}
            />
          ) : (
            <MoneyInput
              ariaLabel={`${m.amount}: ${name}`}
              className="narrow"
              value={payment.amount.fixed}
              onCommit={(value) => onChange({ amount: { fixed: value ?? 0 } })}
            />
          )}
        </span>
      </td>
      <td>
        <AnchorPicker project={project} anchor={payment.anchor} label={`${m.anchor}: ${name}`} onChange={(anchor) => onChange({ anchor })} />
      </td>
      <td>
        <ReadField
          label={`${m.lag}: ${name}`}
          className="tiny"
          title={m.lagTip}
          shown={String(payment.lag_days)}
          read={parseLagDays}
          refusal={m.lagUnreadable}
          onCommit={(value) => onChange({ lag_days: Number(value) })}
        />
      </td>
      <td className="payment-check">
        <input
          type="checkbox"
          aria-label={`${m.retained}: ${name}`}
          title={m.retainedTip}
          checked={payment.retained}
          onChange={(event) => onChange({ retained: event.target.checked })}
        />
      </td>
      <td className="payment-date">
        {dateText(dates?.invoice)}
        {dates?.actual && <span className="meta"> ({m.actualTag})</span>}
      </td>
      <td className="payment-date">{dateText(dates?.due)}</td>
      <td>
        <button type="button" className="link-button" aria-label={m.recordFor(name)} onClick={onReceipts}>
          <Wallet aria-hidden="true" size={13} />
          <span>{payment.received.length > 0 ? m.receiptsCount(payment.received.length) : m.receiptsNone}</span>
        </button>
      </td>
      <td>
        <Button Icon={Trash2} variant="danger" className="btn-icon" aria-label={m.removeNamed(name)} title={m.remove} onClick={onRemove} />
      </td>
    </tr>
  );
}
