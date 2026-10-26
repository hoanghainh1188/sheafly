import { Check, Plus, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { paymentDates } from "../../api";
import { useMessages } from "../../i18n";
import { commitFocusedField } from "../../shortcuts";
import type { CashFlowSettings, Command, PaymentDates, PaymentInput, Project, Receipt, Snapshot } from "../../types";
import { usePendingList } from "../../usePendingList";
import { Button } from "../Button";
import { Modal } from "../Modal";
import { ContractSettings } from "./ContractSettings";
import { DEFAULT_LAG_DAYS, defaultAnchor, formatPercent, percentTotal, toInput } from "./paymentText";
import { PaymentRow } from "./PaymentRow";
import { paymentMessages } from "./payments.i18n";
import { ReceiptsEditor } from "./ReceiptsEditor";

interface Props {
  project: Project;
  changed: boolean;
  /** The last edit the engine refused. */
  error: string | null;
  /** Today at midnight, where a new receipt starts. */
  today: number;
  onSave: () => void;
  onCancel: () => void;
  send: (command: Command) => Promise<Snapshot | void>;
}

/**
 * Dự án → Thiết lập dự án → Lịch thanh toán (Free, phase 25): the contract value, opening
 * balance and retention, and the payments by milestone with the dates the engine gives
 * them (D7). A transaction like Giao việc: edits apply as they are made, Lưu keeps them and
 * Hủy undoes back to where the dialog opened. The list goes whole through `SetPayments`;
 * receipts go through `SetPaymentReceipts`, apart from the terms.
 */
export function PaymentScheduleDialog({ project, changed, error, today, onSave, onCancel, send }: Props) {
  const m = useMessages(paymentMessages);
  const [receiptsFor, setReceiptsFor] = useState<number | null>(null);
  // The engine's dates with the list they were asked for, so a row never shows another's.
  const [dates, setDates] = useState<{ asked: string; dates: (PaymentDates | null)[] } | null>(null);
  // Receipts sent per payment while the engine has not answered, like usePendingList.
  const receiptsSent = useRef(new Map<number, { list: Receipt[]; pending: number }>());
  // Edits still on their way, and whether the last one answered was refused.
  const inFlight = useRef(new Set<Promise<Snapshot | void>>());
  const refused = useRef(false);
  const tracked = useCallback(
    (command: Command) => {
      const sent = send(command).then((state) => {
        refused.current = !state;
        return state;
      });
      inFlight.current.add(sent);
      void sent.finally(() => inFlight.current.delete(sent));
      return sent;
    },
    [send],
  );
  const inputs = useMemo(() => project.payments.map(toInput), [project.payments]);
  const payments = usePendingList<PaymentInput>(inputs, (next) => tracked({ type: "set_payments", payments: next }));
  const stored = useMemo(() => [project.cash_flow], [project.cash_flow]);
  const settings = usePendingList<CashFlowSettings>(stored, ([next]) => tracked({ type: "set_cash_flow_settings", settings: next }));

  // The engine dates every anchor again whenever the schedule changes; older answers are dropped.
  const retention = project.cash_flow.retention;
  const asked = JSON.stringify([project.payments.map((payment) => [payment.id, payment.anchor, payment.lag_days]), retention]);
  useEffect(() => {
    let live = true;
    const items = project.payments.map((payment) => ({ anchor: payment.anchor, lag_days: payment.lag_days }));
    if (retention) items.push({ anchor: retention.release, lag_days: retention.lag_days });
    paymentDates(items)
      .then((answer) => live && setDates({ asked, dates: answer }))
      .catch(() => live && setDates(null));
    return () => {
      live = false;
    };
    // `project` too: a task that moves moves the dates while the terms stay the same.
  }, [project, retention, asked]);
  const shownDates = dates?.asked === asked ? dates.dates : null;

  function changeReceipts(id: number, stored: Receipt[], update: (receipts: Receipt[]) => Receipt[]) {
    const entry = receiptsSent.current.get(id);
    // In date order, as the engine keeps them, so a row's place matches what is drawn next.
    const next = [...update(entry && entry.pending > 0 ? entry.list : stored)].sort((a, b) => a.date - b.date);
    const sent = { list: next, pending: (entry?.pending ?? 0) + 1 };
    receiptsSent.current.set(id, sent);
    void tracked({ type: "set_payment_receipts", id, receipts: next }).then((state) => {
      sent.pending -= 1;
      // Refused: the next edit starts from what the engine holds.
      if (!state) sent.pending = 0;
    });
  }

  function change(id: number, edit: Partial<PaymentInput>) {
    void payments.apply(payments.latest().map((input) => (input.id === id ? { ...input, ...edit } : input)));
  }

  function add() {
    const list = payments.latest();
    const contract = (settings.latest()[0] ?? project.cash_flow).contract_value != null;
    void payments.apply([
      ...list,
      {
        name: m.newPayment(list.length + 1),
        amount: contract ? { percent: 0 } : { fixed: 0 },
        anchor: defaultAnchor(project),
        lag_days: DEFAULT_LAG_DAYS,
        retained: project.cash_flow.retention != null,
      },
    ]);
  }

  // Lưu leaves the field being typed in first, and closes only once the engine answered
  // every edit; when the last one was refused the dialog stays, the reason in the footer.
  async function save() {
    commitFocusedField();
    const [listOk, settingsOk] = [await payments.settled(), await settings.settled()];
    while (inFlight.current.size > 0) await Promise.all([...inFlight.current]);
    const ok = listOk && settingsOk && !refused.current;
    refused.current = false;
    if (ok) onSave();
  }

  const total = percentTotal(project.payments);
  const receiptsPayment = project.payments.find((payment) => payment.id === receiptsFor);
  return (
    <Modal
      title={m.title}
      className="wide payment-dialog"
      onClose={onCancel}
      footer={
        <>
          {error ? (
            <span className="dialog-foot-note bad" role="alert">
              {error}
            </span>
          ) : (
            <span className="meta dialog-foot-note">{changed ? m.previewing : ""}</span>
          )}
          <Button Icon={X} onClick={onCancel}>{m.cancel}</Button>
          <Button variant="primary" Icon={Check} onClick={() => void save()}>{m.save}</Button>
        </>
      }
    >
      <ContractSettings
        project={project}
        settings={project.cash_flow}
        releaseDates={retention ? shownDates?.[project.payments.length] : undefined}
        onChange={(update) => void settings.apply([update(settings.latest()[0] ?? project.cash_flow)])}
      />
      <section className="payment-list" aria-labelledby="payment-list-title">
        <h3 id="payment-list-title">{m.payments}</h3>
        <p className="meta">{m.paymentsHint}</p>
        {project.payments.length === 0 ? (
          <p className="payment-empty">{m.none}</p>
        ) : (
          <div className="payment-table-scroll">
            <table className="payment-table">
              <thead>
                <tr>
                  <th scope="col">{m.name}</th>
                  <th scope="col">{m.amount}</th>
                  <th scope="col">{m.anchor}</th>
                  <th scope="col" title={m.lagTip}>{m.lag}</th>
                  <th scope="col" title={m.retainedTip}>{m.retained}</th>
                  <th scope="col">{m.invoice}</th>
                  <th scope="col">{m.due}</th>
                  <th scope="col">{m.receipts}</th>
                  <th scope="col"><span className="visually-hidden">{m.remove}</span></th>
                </tr>
              </thead>
              <tbody>
                {project.payments.map((payment, index) => (
                  <PaymentRow
                    key={payment.id}
                    project={project}
                    payment={payment}
                    dates={shownDates?.[index]}
                    onChange={(edit) => change(payment.id, edit)}
                    onRemove={() => {
                      if (receiptsFor === payment.id) setReceiptsFor(null);
                      void payments.apply(payments.latest().filter((input) => input.id !== payment.id));
                    }}
                    onReceipts={() => setReceiptsFor(payment.id)}
                  />
                ))}
              </tbody>
            </table>
          </div>
        )}
        {total != null && (
          <p className={total === 100 ? "meta" : "payment-warning"} role={total === 100 ? undefined : "status"}>
            {total === 100 ? m.percentTotal(formatPercent(total)) : m.percentTotalOff(formatPercent(total))}
          </p>
        )}
        {total != null && project.cash_flow.contract_value == null && <p className="payment-warning">{m.percentNoContract}</p>}
        <div>
          <Button Icon={Plus} onClick={add}>{m.add}</Button>
        </div>
      </section>
      {receiptsPayment && (
        <ReceiptsEditor
          key={receiptsPayment.id}
          payment={receiptsPayment}
          today={today}
          onChange={(update) => changeReceipts(receiptsPayment.id, receiptsPayment.received, update)}
          onClose={() => setReceiptsFor(null)}
        />
      )}
    </Modal>
  );
}
