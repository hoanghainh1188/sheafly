import { formatDateOnly, formatMoney } from "../../format";
import { useMessages } from "../../i18n";
import type { CashFlowNote, CashFlowReport, PaymentAnchor, PaymentForecast, Project } from "../../types";
import { formatPercent } from "../payments/paymentText";
import { cashFlowMessages } from "./CashFlowView.i18n";

type Messages = (typeof cashFlowMessages)["vi"];

interface Props {
  project: Project;
  report: CashFlowReport;
  /** Select a task and show it on the Gantt. */
  onPickTask: (uid: number) => void;
}

function anchorOf(project: Project, item: PaymentForecast): PaymentAnchor | null {
  if (item.id == null) return project.cash_flow.retention?.release ?? null;
  return project.payments.find((payment) => payment.id === item.id)?.anchor ?? null;
}

function TaskButton({ project, uid, onPickTask }: { project: Project; uid: number; onPickTask: (uid: number) => void }) {
  const m = useMessages(cashFlowMessages);
  const task = project.tasks.find((item) => item.uid === uid);
  if (!task) return null;
  const name = task.name.trim() || `#${task.id}`;
  return (
    <button type="button" className="link-button" title={m.pickTask(name)} onClick={() => onPickTask(uid)}>
      {name}
    </button>
  );
}

function lateText(m: Messages, days: number | null): string {
  if (days == null) return m.noValue;
  if (days > 0) return m.lateDays(days);
  if (days < 0) return m.early(-days);
  return m.onTime;
}

function noteText(m: Messages, project: Project, note: CashFlowNote): string {
  if (note.type === "percent_total") return m.notePercentTotal(formatPercent(note.total));
  const name = project.payments.find((payment) => payment.id === note.id)?.name.trim() || m.unknownPayment(note.id);
  if (note.type === "percent_without_contract") return m.notePercentWithoutContract(name);
  if (note.type === "over_received") return m.noteOverReceived(name);
  return m.noteDetached(name);
}

/**
 * Each payment as the engine forecasts it: due as scheduled against the baseline, how late,
 * what is overdue, and the tasks driving its milestone. A milestone's name selects it.
 */
export function CashFlowPayments({ project, report, onPickTask }: Props) {
  const m = useMessages(cashFlowMessages);
  const date = (at: number | null | undefined) => (at == null ? m.noValue : formatDateOnly(at));
  return (
    <section className="cash-payments" aria-labelledby="cash-payments-title">
      <h3 id="cash-payments-title">{m.payments}</h3>
      {report.payments.length === 0 ? (
        <p className="meta">{m.paymentsNone}</p>
      ) : (
        <div className="cash-table-scroll">
          <table className="cash-table">
            <thead>
              <tr>
                <th scope="col">{m.colName}</th>
                <th scope="col" className="text">{m.colAnchor}</th>
                <th scope="col">{m.colAmount}</th>
                <th scope="col">{m.colReceived}</th>
                <th scope="col">{m.colDue}</th>
                {report.baseline != null && <th scope="col">{m.colBaselineDue}</th>}
                {report.baseline != null && <th scope="col">{m.colLate}</th>}
              </tr>
            </thead>
            <tbody>
              {report.payments.map((item, index) => {
                const anchor = anchorOf(project, item);
                return (
                  <tr key={item.id ?? `release-${index}`} className={item.overdue ? "overdue" : undefined}>
                    <th scope="row">
                      {item.id == null ? m.retentionRelease : item.name}
                      {item.overdue && (
                        <span className="cash-tag" title={item.expected != null ? m.overdueTip(date(item.expected)) : undefined}>
                          {m.overdue}
                        </span>
                      )}
                      {item.driving.length > 0 && (
                        <span className="cash-driving">
                          {m.driving}:{" "}
                          {item.driving.map((uid, at) => (
                            <span key={uid}>
                              {at > 0 && ", "}
                              <TaskButton project={project} uid={uid} onPickTask={onPickTask} />
                            </span>
                          ))}
                        </span>
                      )}
                    </th>
                    <td className="text">
                      {anchor && anchor.type !== "date" ? <TaskButton project={project} uid={anchor.task} onPickTask={onPickTask} /> : m.fixedDate}
                    </td>
                    <td>{formatMoney(item.amount)}</td>
                    <td>{formatMoney(item.received)}</td>
                    <td>{date(item.scheduled?.due)}</td>
                    {report.baseline != null && <td>{date(item.baseline?.due)}</td>}
                    {report.baseline != null && (
                      <td className={item.late_days != null && item.late_days > 0 ? "negative" : undefined}>{lateText(m, item.late_days)}</td>
                    )}
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
      {report.notes.length > 0 && (
        <div className="cash-notes" role="note" aria-label={m.notes}>
          <strong>{m.notes}</strong>
          <ul>
            {report.notes.map((note, index) => (
              <li key={`${note.type}-${index}`}>{noteText(m, project, note)}</li>
            ))}
          </ul>
        </div>
      )}
    </section>
  );
}
