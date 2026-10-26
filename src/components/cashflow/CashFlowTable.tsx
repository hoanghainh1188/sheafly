import { formatMoney } from "../../format";
import { useMessages } from "../../i18n";
import type { CashFlowReport } from "../../types";
import { figures, periodLabel, type CashSeries } from "./cashFlowData";
import { cashFlowMessages } from "./CashFlowView.i18n";

/** The figures of every period in one series; a period short of money is marked in words too. */
export function CashFlowTable({ report, series }: { report: CashFlowReport; series: CashSeries }) {
  const m = useMessages(cashFlowMessages);
  const show = (value: number | null) => (value == null ? m.noValue : formatMoney(value));
  return (
    <section className="cash-table-wrap" aria-labelledby="cash-table-title">
      <h3 id="cash-table-title">{m.table}</h3>
      <div className="cash-table-scroll">
        <table className="cash-table">
          <thead>
            <tr>
              <th scope="col">{m.colPeriod}</th>
              <th scope="col">{m.inflow}</th>
              <th scope="col">{m.outflow}</th>
              <th scope="col">{m.net}</th>
              <th scope="col">{m.balance}</th>
            </tr>
          </thead>
          <tbody>
            {report.periods.map((period) => {
              const f = figures(period, series);
              // Short of money is the forecast balance's, so only the forecast marks it.
              const deficit = period.deficit && series === "forecast";
              return (
                <tr key={period.label} className={deficit ? "deficit" : undefined}>
                  <th scope="row">
                    {periodLabel(period.label, report.bucket)}
                    {deficit && <span className="cash-tag">{m.deficitTag}</span>}
                  </th>
                  <td>{show(f.inflow)}</td>
                  <td>{show(f.outflow)}</td>
                  <td className={f.net != null && f.net < 0 ? "negative" : undefined}>{show(f.net)}</td>
                  <td className={f.balance != null && f.balance < 0 ? "negative" : undefined}>{show(f.balance)}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </section>
  );
}
