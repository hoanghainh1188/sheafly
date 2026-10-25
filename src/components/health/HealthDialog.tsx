import { Filter, X } from "lucide-react";

import { useMessages } from "../../i18n";
import type { HealthCriterion, HealthReport, HealthStatus } from "../../types";
import { Button } from "../Button";
import { ProBadge } from "../license/ProBadge";
import { Modal } from "../Modal";
import { healthMessages } from "./HealthDialog.i18n";
import { formatHealthLimit, formatHealthValue, reasonCounts } from "./healthFormat";

interface HealthDialogProps {
  /** The last report, possibly for an older version of the project. */
  report: HealthReport | null;
  /** Whether `report` is for the project as it is now. */
  current: boolean;
  error: string | null;
  /** Filter the task sheet by a criterion's tasks (the dialog closes). */
  onPick: (number: number) => void;
  onClose: () => void;
}

/**
 * Dự án → Kiểm tra chất lượng lịch (phase 23, Pro `health`): DCMA's 14 points as the Pro
 * crate scores them, with Sheafly's overall rating. Picking a criterion filters the task
 * sheet by its tasks. Read only: nothing here changes the project.
 */
export function HealthDialog({ report, current, error, onPick, onClose }: HealthDialogProps) {
  const m = useMessages(healthMessages);
  return (
    <Modal
      title={
        <>
          {m.title}
          <ProBadge />
        </>
      }
      className="wide health-dialog"
      onClose={onClose}
      footer={
        <>
          <span className="dialog-foot-note bad" role="alert">
            {error ?? ""}
          </span>
          <span className="meta dialog-foot-note" role="status">
            {report == null && error == null ? m.loading : report != null && !current ? m.stale : ""}
          </span>
          <span className="spacer" />
          <Button Icon={X} onClick={onClose}>
            {m.close}
          </Button>
        </>
      }
    >
      <div className={`health${current ? "" : " stale"}`}>
        <p className="meta">{m.intro}</p>
        {report && <Summary report={report} />}
        {report && (
          <table className="health-table">
            <thead>
              <tr>
                <th scope="col">{m.colNumber}</th>
                <th scope="col">{m.colCriterion}</th>
                <th scope="col">{m.colResult}</th>
                <th scope="col">{m.colLimit}</th>
                <th scope="col">{m.colStatus}</th>
                <th scope="col" aria-label={m.colCriterion} />
              </tr>
            </thead>
            <tbody>
              {report.criteria.map((criterion) => (
                <CriterionRow key={criterion.number} criterion={criterion} onPick={onPick} />
              ))}
            </tbody>
          </table>
        )}
      </div>
    </Modal>
  );
}

function Summary({ report }: { report: HealthReport }) {
  const m = useMessages(healthMessages);
  return (
    <section className="health-summary" aria-live="polite">
      {report.rating ? (
        <span className={`health-rating ${report.rating}`}>{m.rating(report.rating)}</span>
      ) : null}
      <strong>{report.rating ? m.summary(report.passed, report.applicable) : m.nothingApplies}</strong>
      <span className="meta">{m.ratingNote}</span>
    </section>
  );
}

function CriterionRow({ criterion, onPick }: { criterion: HealthCriterion; onPick: (number: number) => void }) {
  const m = useMessages(healthMessages);
  const { number, status, limit, tasks } = criterion;
  const name = m.name(number);
  const reasons = reasonCounts(criterion)
    .map(([reason, count]) => `${count} ${m.reason(reason)}`)
    .join(", ");
  return (
    <tr className={`health-row ${status}`}>
      <td className="num">{number}</td>
      <th scope="row">
        <span className="health-name">{name}</span>
        <span className="meta health-hint">{m.hint(number)}</span>
      </th>
      <td>
        <Result criterion={criterion} />
        {reasons && <span className="meta health-reasons">{reasons}</span>}
      </td>
      <td className="num">{limit ? formatHealthLimit(number, limit) : ""}</td>
      <td>
        <span className={`health-status ${status}`}>{statusLabels(m)[status]}</span>
      </td>
      <td>
        {tasks.length > 0 && (
          <Button Icon={Filter} aria-label={m.showAria(tasks.length, name)} onClick={() => onPick(number)}>
            {m.show(tasks.length)}
          </Button>
        )}
      </td>
    </tr>
  );
}

function Result({ criterion }: { criterion: HealthCriterion }) {
  const m = useMessages(healthMessages);
  const { number, status, value, count, total } = criterion;
  if (criterion.not_applicable) return <span>{m.notApplicable(criterion.not_applicable)}</span>;
  if (number === 12) return <span>{status === "pass" ? m.passedTest : m.failedTest}</span>;
  const shown = value == null ? "" : formatHealthValue(number, value);
  if (number === 13) return <span className="num">{shown}</span>;
  return (
    <span className="num">
      {shown} <span className="meta">({m.counted(count, total)})</span>
    </span>
  );
}

/** The status words, by status. */
function statusLabels(m: { pass: string; fail: string; info: string; notApplicableStatus: string }): Record<HealthStatus, string> {
  return { pass: m.pass, fail: m.fail, info: m.info, not_applicable: m.notApplicableStatus };
}
