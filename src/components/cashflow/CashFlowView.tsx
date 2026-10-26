import { Banknote, KeyRound } from "lucide-react";
import { useMemo, useState } from "react";

import { formatMoney } from "../../format";
import { useMessages } from "../../i18n";
import type { Bucket, Project } from "../../types";
import { Button } from "../Button";
import { ProBadge } from "../license/ProBadge";
import { CashFlowChart } from "./CashFlowChart";
import { CashFlowPayments } from "./CashFlowPayments";
import { CashFlowTable } from "./CashFlowTable";
import { periodLabel, type CashSeries } from "./cashFlowData";
import { cashFlowMessages } from "./CashFlowView.i18n";
import { useCashFlow } from "./useCashFlow";

const BUCKETS: Bucket[] = ["day", "week", "month", "quarter"];

interface Props {
  project: Project;
  /** Whether this computer's license includes the cash flow. */
  cashflowPro: boolean;
  onOpenLicense: () => void;
  onOpenPayments: () => void;
  /** Select a task and show it on the Gantt. */
  onPickTask: (uid: number) => void;
}

/**
 * Xem → Dòng tiền (Pro `cashflow`, phase 25): money in and out by period, the running
 * balance, the periods short of money and each payment against the baseline, all computed
 * by the engine. Without the license it introduces the feature; the payment schedule it
 * reads stays Free.
 */
export function CashFlowView({ project, cashflowPro, onOpenLicense, onOpenPayments, onPickTask }: Props) {
  const m = useMessages(cashFlowMessages);
  const [bucket, setBucket] = useState<Bucket>("month");
  const baselines = useMemo(
    () => [...new Set(project.tasks.flatMap((task) => task.baselines.map((b) => b.number)))].sort((a, b) => a - b),
    [project.tasks],
  );
  const [chosen, setChosen] = useState<number | null>(null);
  // A baseline cleared while chosen falls back to none.
  const baseline = chosen != null && baselines.includes(chosen) ? chosen : null;
  const [picked, setPicked] = useState<CashSeries>("forecast");
  const series: CashSeries = picked === "baseline" && baseline == null ? "forecast" : picked;
  const { report, current, error } = useCashFlow(project, cashflowPro, bucket, baseline);
  const seriesName = series === "forecast" ? m.forecast : series === "actual" ? m.actual : m.baselineSeries;

  if (!cashflowPro) {
    return (
      <section className="cash-view" aria-label={m.region}>
        <div className="cash-intro">
          <strong>
            {m.introTitle}
            <ProBadge />
          </strong>
          <p>{m.intro}</p>
          <div className="cash-actions">
            <Button variant="primary" Icon={KeyRound} onClick={onOpenLicense}>{m.activate}</Button>
            <Button Icon={Banknote} onClick={onOpenPayments}>{m.openPayments}</Button>
          </div>
        </div>
      </section>
    );
  }

  const firstDeficit = report?.periods.find((period) => period.date === report.first_deficit);
  return (
    <section className="cash-view" aria-label={m.region} aria-busy={!current}>
      <div className="cash-toolbar">
        <div className="segmented" role="group" aria-label={m.period}>
          {BUCKETS.map((id) => (
            <button key={id} type="button" className={bucket === id ? "on" : ""} aria-pressed={bucket === id} onClick={() => setBucket(id)}>
              {m[id]}
            </button>
          ))}
        </div>
        <label className="cash-control">
          <span>{m.baseline}</span>
          <select value={baseline ?? ""} onChange={(event) => setChosen(event.target.value === "" ? null : Number(event.target.value))}>
            <option value="">{m.baselineNone}</option>
            {baselines.map((n) => (
              <option key={n} value={n}>{m.baselineNumber(n)}</option>
            ))}
          </select>
        </label>
        <div className="segmented" role="group" aria-label={m.series}>
          {(["forecast", "actual", "baseline"] as CashSeries[]).map((id) => (
            <button
              key={id}
              type="button"
              className={series === id ? "on" : ""}
              aria-pressed={series === id}
              disabled={id === "baseline" && baseline == null}
              title={id === "baseline" && baseline == null ? m.baselineSeriesOff : undefined}
              onClick={() => setPicked(id)}
            >
              {id === "forecast" ? m.forecast : id === "actual" ? m.actual : m.baselineSeries}
            </button>
          ))}
        </div>
        <span className="spacer" />
        <Button Icon={Banknote} onClick={onOpenPayments}>{m.openPayments}</Button>
      </div>
      {error != null ? (
        <p className="cash-empty" role="alert">{m.failed(error)}</p>
      ) : !report ? (
        <p className="cash-empty">{m.loading}</p>
      ) : report.periods.length === 0 ? (
        <p className="cash-empty">{m.empty}</p>
      ) : (
        <div className={current ? "cash-content" : "cash-content stale"}>
          {!current && <p className="meta" role="status">{m.stale}</p>}
          <dl className="cash-summary">
            <div>
              <dt>{m.opening}</dt>
              <dd>{formatMoney(report.opening_balance)}</dd>
            </div>
            <div>
              <dt>{m.lowest}</dt>
              <dd className={report.lowest_balance != null && report.lowest_balance < 0 ? "negative" : undefined}>
                {report.lowest_balance == null ? m.noValue : formatMoney(report.lowest_balance)}
              </dd>
            </div>
            <div className={firstDeficit ? "cash-alert" : undefined}>
              <dt>{firstDeficit ? m.firstDeficit : m.deficit}</dt>
              <dd>{firstDeficit ? periodLabel(firstDeficit.label, report.bucket) : m.noDeficit}</dd>
            </div>
          </dl>
          <CashFlowChart report={report} series={series} seriesName={seriesName} />
          <CashFlowPayments project={project} report={report} onPickTask={onPickTask} />
          <CashFlowTable report={report} series={series} />
        </div>
      )}
    </section>
  );
}
