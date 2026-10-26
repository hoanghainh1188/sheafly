import { Check, Plus, X } from "lucide-react";
import { useState } from "react";

import { formatMoney } from "../../format";
import { useMessages } from "../../i18n";
import type { Payment, Receipt } from "../../types";
import { Button } from "../Button";
import { PickerField } from "../DateTimeField";
import { MoneyInput } from "../MoneyInput";
import { paymentMessages } from "./payments.i18n";

interface Props {
  payment: Payment;
  /** Today at midnight, where a new receipt starts. */
  today: number;
  /**
   * Replaces the payment's receipts (`SetPaymentReceipts`); the engine keeps them by date.
   * Applied to the list sent last, so quick edits in a row all count.
   */
  onChange: (update: (receipts: Receipt[]) => Receipt[]) => void;
  onClose: () => void;
}

/**
 * Money received for one payment, in as many parts as it came. Recorded apart from the
 * terms, so tracking what came in never touches the schedule.
 */
export function ReceiptsEditor({ payment, today, onChange, onClose }: Props) {
  const m = useMessages(paymentMessages);
  const [date, setDate] = useState(today);
  const [amount, setAmount] = useState<number | null>(null);
  const name = payment.name.trim() || "—";
  const replace = (index: number, edit: ((receipt: Receipt) => Receipt) | null) =>
    onChange((receipts) => receipts.flatMap((item, i) => (i !== index ? [item] : edit ? [edit(item)] : [])));
  return (
    <section className="payment-receipts" aria-label={m.receiptsTitle(name)}>
      <h3>{m.receiptsTitle(name)}</h3>
      {payment.received.length > 0 && (
        <ul>
          {payment.received.map((receipt, index) => (
            // By place, so editing a date does not remount the row and lose focus.
            <li key={index}>
              <PickerField
                mode="date"
                value={receipt.date}
                ariaLabel={`${m.receiptDate} ${index + 1}`}
                onCommit={(at) => at != null && replace(index, (item) => ({ ...item, date: at }))}
              />
              <MoneyInput
                ariaLabel={`${m.receiptAmount} ${index + 1}`}
                className="narrow"
                value={receipt.amount}
                onCommit={(value) => replace(index, (item) => ({ ...item, amount: value ?? 0 }))}
              />
              <Button Icon={X} className="btn-icon" aria-label={`${m.removeReceipt} ${index + 1}`} onClick={() => replace(index, null)} />
            </li>
          ))}
        </ul>
      )}
      <div className="payment-receipt-new">
        <PickerField mode="date" value={date} ariaLabel={m.receiptDate} onCommit={(at) => at != null && setDate(at)} />
        <MoneyInput ariaLabel={m.receiptAmount} className="narrow" optional value={amount} placeholder={formatMoney(0)} onCommit={setAmount} />
        <Button
          Icon={Plus}
          disabled={amount == null || amount <= 0}
          onClick={() => {
            if (amount == null) return;
            onChange((receipts) => [...receipts, { date, amount }]);
            setAmount(null);
          }}
        >
          {m.addReceipt}
        </Button>
        <Button Icon={Check} onClick={onClose}>
          {m.closeReceipts}
        </Button>
      </div>
    </section>
  );
}
