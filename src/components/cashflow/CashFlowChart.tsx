import { formatMoney } from "../../format";
import { useMessages } from "../../i18n";
import type { CashFlowReport } from "../../types";
import { formatCompactMoney } from "../EvmPanel";
import { figures, periodLabel, ticks, valueRange, type CashSeries } from "./cashFlowData";
import { cashFlowMessages } from "./CashFlowView.i18n";

interface Props {
  report: CashFlowReport;
  series: CashSeries;
  seriesName: string;
}

const PAD = { top: 16, right: 20, bottom: 40, left: 72 };
const WIDTH = 920;
const HEIGHT = 340;
// At most this many period names along the axis; the rest are in each column's title.
const MAX_LABELS = 12;

/**
 * Money in above zero and money out below it, one pair of bars a period, with the running
 * balance as a line. Periods whose forecast balance is below zero are shaded.
 */
export function CashFlowChart({ report, series, seriesName }: Props) {
  const m = useMessages(cashFlowMessages);
  const periods = report.periods;
  const range = valueRange(report, series);
  // A little room past the tallest bar and the lowest point, so neither touches the frame.
  const room = (range.high - range.low) * 0.06;
  const [low, high] = [range.low < 0 ? range.low - room : 0, range.high + room];
  const plotW = WIDTH - PAD.left - PAD.right;
  const plotH = HEIGHT - PAD.top - PAD.bottom;
  const column = plotW / periods.length;
  const y = (value: number) => PAD.top + ((high - value) / (high - low)) * plotH;
  const barW = Math.max(1.5, Math.min(22, column * 0.36));
  const every = Math.ceil(periods.length / MAX_LABELS);
  const show = (value: number | null) => (value == null ? m.noValue : formatMoney(value));
  const points = periods.flatMap((period, index) => {
    const balance = figures(period, series).balance;
    return balance == null ? [] : [`${(PAD.left + column * (index + 0.5)).toFixed(1)},${y(balance).toFixed(1)}`];
  });
  return (
    <figure className="cash-figure">
      <svg className="cash-svg" viewBox={`0 0 ${WIDTH} ${HEIGHT}`} role="img" aria-label={m.chart(seriesName)}>
        <rect x={PAD.left} y={PAD.top} width={plotW} height={plotH} style={{ fill: "var(--chart-bg)", stroke: "var(--chart-frame)" }} />
        {/* Short of money is the forecast balance's: shaded only while the forecast is shown. */}
        {periods.map((period, index) =>
          period.deficit && series === "forecast" ? (
            <rect
              key={`deficit-${period.label}`}
              className="cash-deficit"
              x={PAD.left + column * index}
              y={PAD.top}
              width={column}
              height={plotH}
              style={{ fill: "var(--cash-deficit-bg)" }}
            />
          ) : null,
        )}
        {ticks(low, high).map((tick) => (
          <g key={`tick-${tick}`}>
            <line
              x1={PAD.left}
              x2={PAD.left + plotW}
              y1={y(tick)}
              y2={y(tick)}
              style={{ stroke: tick === 0 ? "var(--chart-frame)" : "var(--chart-grid)" }}
              strokeWidth={tick === 0 ? 1.5 : 1}
            />
            <text x={PAD.left - 8} y={y(tick) + 4} textAnchor="end" fontSize="10" style={{ fill: "var(--text-muted)" }}>
              {formatCompactMoney(tick)}
            </text>
          </g>
        ))}
        {periods.map((period, index) => {
          const f = figures(period, series);
          const center = PAD.left + column * (index + 0.5);
          return (
            <g key={period.label}>
              <title>{m.barTitle(periodLabel(period.label, report.bucket), show(f.inflow), show(f.outflow), show(f.balance))}</title>
              {f.inflow != null && f.inflow > 0 && (
                <rect x={center - barW} y={y(f.inflow)} width={barW} height={y(0) - y(f.inflow)} style={{ fill: "var(--cash-in)" }} rx={1} />
              )}
              {f.outflow != null && f.outflow > 0 && (
                <rect x={center} y={y(0)} width={barW} height={y(-f.outflow) - y(0)} style={{ fill: "var(--cash-out)" }} rx={1} />
              )}
              {/* The whole column answers the pointer, so its title shows over gaps too. */}
              <rect x={PAD.left + column * index} y={PAD.top} width={column} height={plotH} fill="transparent" />
              {index % every === 0 && (
                <text x={center} y={HEIGHT - PAD.bottom + 16} textAnchor="middle" fontSize="10" style={{ fill: "var(--text-muted)" }}>
                  {periodLabel(period.label, report.bucket)}
                </text>
              )}
            </g>
          );
        })}
        {points.length > 1 && (
          <polyline points={points.join(" ")} fill="none" style={{ stroke: "var(--cash-net)" }} strokeWidth={2.25} strokeLinejoin="round" />
        )}
        {points.length === 1 && <circle cx={Number(points[0].split(",")[0])} cy={Number(points[0].split(",")[1])} r={3} style={{ fill: "var(--cash-net)" }} />}
      </svg>
      <figcaption className="cash-legend">
        <span><i className="cash-swatch" style={{ background: "var(--cash-in)" }} /> {m.inflow}</span>
        <span><i className="cash-swatch" style={{ background: "var(--cash-out)" }} /> {m.outflow}</span>
        <span><i className="cash-swatch line" style={{ background: "var(--cash-net)" }} /> {m.balance}</span>
        {series === "forecast" && (
          <span><i className="cash-swatch" style={{ background: "var(--cash-deficit-bg)", borderColor: "var(--cash-deficit)" }} /> {m.deficit}</span>
        )}
      </figcaption>
    </figure>
  );
}
