import { useCallback, useEffect, useId, useMemo, useState } from "react";

import { curves } from "../../api";
import { formatCompactMoney } from "../EvmPanel";
import { useMessages } from "../../i18n";
import type { CurveBucket, CurveFilter, CurveMeasure, CurvePoint, CurvesReport, Project } from "../../types";
import { curvesMessages } from "./CurvesView.i18n";

interface Props {
  project: Project;
}

type DisplayMode = "cumulative" | "periodic";

const CHART_PADDING = { top: 24, right: 32, bottom: 48, left: 64 };
const CHART_WIDTH = 920;
const CHART_HEIGHT = 380;

export function CurvesView({ project }: Props) {
  const m = useMessages(curvesMessages);

  // Filter & configuration state
  const [measure, setMeasure] = useState<CurveMeasure>("cost");
  const [bucket, setBucket] = useState<CurveBucket>("week");
  const [filterType, setFilterType] = useState<"project" | "person" | "group" | "skill">("project");
  const [personUid, setPersonUid] = useState<number>(() => project.resources[0]?.uid ?? 0);
  const [groupName, setGroupName] = useState<string>(() => {
    const firstGroup = project.resources.find((r) => (r.group ?? "").trim().length > 0)?.group;
    return firstGroup ?? "";
  });
  const [skillId, setSkillId] = useState<number>(() => project.skills[0]?.id ?? 0);
  const [minLevel, setMinLevel] = useState<number | null>(null);

  const availableBaselines = useMemo(() => {
    const nums = new Set<number>();
    for (const t of project.tasks) {
      for (const b of t.baselines) {
        nums.add(b.number);
      }
    }
    return Array.from(nums).sort((a, b) => a - b);
  }, [project.tasks]);

  const [baselineNum, setBaselineNum] = useState<number | null>(() =>
    availableBaselines.includes(0) ? 0 : availableBaselines[0] ?? null,
  );

  const [mode, setMode] = useState<DisplayMode>("cumulative");
  const [report, setReport] = useState<CurvesReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [hoveredIdx, setHoveredIdx] = useState<number | null>(null);

  // Groups and Skills dropdown data
  const distinctGroups = useMemo(() => {
    const groups = new Set<string>();
    for (const r of project.resources) {
      const g = (r.group ?? "").trim();
      if (g.length > 0) {
        groups.add(r.group ?? g);
      }
    }
    return Array.from(groups).sort();
  }, [project.resources]);

  // Build active filter
  const currentFilter: CurveFilter = useMemo(() => {
    switch (filterType) {
      case "project":
        return { type: "project" };
      case "person":
        return { type: "person", uid: personUid };
      case "group":
        return { type: "group", name: groupName };
      case "skill":
        return { type: "skill", id: skillId, min_level: minLevel ?? undefined };
    }
  }, [filterType, personUid, groupName, skillId, minLevel]);

  const loadData = useCallback(async () => {
    setLoading(true);
    try {
      const rep = await curves(measure, bucket, currentFilter, baselineNum ?? undefined);
      setReport(rep);
    } catch {
      setReport(null);
    } finally {
      setLoading(false);
    }
  }, [measure, bucket, currentFilter, baselineNum]);

  useEffect(() => {
    void loadData();
  }, [loadData]);

  const formatValue = useCallback(
    (val: number | null | undefined): string => {
      if (val == null) return "—";
      if (measure === "cost") return formatCompactMoney(val);
      if (measure === "work") return `${val.toFixed(1)}h`;
      return `${val.toFixed(1)}%`;
    },
    [measure],
  );

  return (
    <div className="curves-view" data-testid="curves-view">
      {/* Controls Bar */}
      <div className="curves-toolbar">
        {/* Measure */}
        <label className="curves-control">
          <span>{m.measure}</span>
          <select value={measure} onChange={(e) => setMeasure(e.target.value as CurveMeasure)} aria-label={m.measure}>
            <option value="cost">{m.measureCost}</option>
            <option value="work">{m.measureWork}</option>
            <option value="progress">{m.measureProgress}</option>
          </select>
        </label>

        {/* Period */}
        <label className="curves-control">
          <span>{m.period}</span>
          <select value={bucket} onChange={(e) => setBucket(e.target.value as CurveBucket)} aria-label={m.period}>
            <option value="day">{m.periodDay}</option>
            <option value="week">{m.periodWeek}</option>
            <option value="month">{m.periodMonth}</option>
          </select>
        </label>

        {/* Filter Type */}
        <label className="curves-control">
          <span>{m.filter}</span>
          <select
            value={filterType}
            onChange={(e) => setFilterType(e.target.value as "project" | "person" | "group" | "skill")}
            aria-label={m.filter}
          >
            <option value="project">{m.filterProject}</option>
            <option value="person">{m.filterPerson}</option>
            <option value="group">{m.filterGroup}</option>
            <option value="skill">{m.filterSkill}</option>
          </select>
        </label>

        {/* Filter sub-controls */}
        {filterType === "person" && (
          <select
            value={personUid}
            onChange={(e) => setPersonUid(Number(e.target.value))}
            aria-label={m.pickPerson}
          >
            {project.resources.map((r) => (
              <option key={r.uid} value={r.uid}>
                {r.name}
              </option>
            ))}
          </select>
        )}

        {filterType === "group" && (
          <select
            value={groupName}
            onChange={(e) => setGroupName(e.target.value)}
            aria-label={m.pickGroup}
          >
            {distinctGroups.map((g) => (
              <option key={g} value={g}>
                {g}
              </option>
            ))}
          </select>
        )}

        {filterType === "skill" && (
          <>
            <select
              value={skillId}
              onChange={(e) => setSkillId(Number(e.target.value))}
              aria-label={m.pickSkill}
            >
              {project.skills.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.name}
                </option>
              ))}
            </select>
            {(project.skill_levels?.length ?? 0) > 0 && (
              <label className="curves-control">
                <span>{m.minLevel}</span>
                <select
                  value={minLevel ?? ""}
                  onChange={(e) => setMinLevel(e.target.value === "" ? null : Number(e.target.value))}
                  aria-label={m.minLevel}
                >
                  <option value="">{m.anyLevel}</option>
                  {project.skill_levels?.map((lvl) => (
                    <option key={lvl.id} value={lvl.id}>
                      {lvl.name}
                    </option>
                  ))}
                </select>
              </label>
            )}
          </>
        )}

        {/* Baseline Selector */}
        <label className="curves-control">
          <span>{m.baseline}</span>
          <select
            value={baselineNum ?? ""}
            onChange={(e) => setBaselineNum(e.target.value === "" ? null : Number(e.target.value))}
            aria-label={m.baseline}
          >
            <option value="">{m.baselineNone}</option>
            {availableBaselines.map((n) => (
              <option key={n} value={n}>
                {m.baselineNumber(n)}
              </option>
            ))}
          </select>
        </label>

        {/* Display Mode Toggle */}
        <div className="curves-mode-toggle" role="group" aria-label={m.mode}>
          <button
            type="button"
            className={`btn-subtle ${mode === "cumulative" ? "active" : ""}`}
            onClick={() => setMode("cumulative")}
          >
            {m.modeCumulative}
          </button>
          <button
            type="button"
            className={`btn-subtle ${mode === "periodic" ? "active" : ""}`}
            onClick={() => setMode("periodic")}
          >
            {m.modePeriodic}
          </button>
        </div>
      </div>

      {/* Main Content Area */}
      {loading ? (
        <div className="curves-empty">{m.loading}</div>
      ) : !report || report.points.length === 0 ? (
        <div className="curves-empty">{m.empty}</div>
      ) : (
        <div className="curves-content">
          {/* Chart Figure */}
          <CurvesChart
            report={report}
            mode={mode}
            measure={measure}
            formatValue={formatValue}
            hoveredIdx={hoveredIdx}
            onHover={setHoveredIdx}
            messages={m}
          />

          {/* Detailed Data Table */}
          <CurvesTable report={report} mode={mode} formatValue={formatValue} messages={m} />
        </div>
      )}
    </div>
  );
}

interface ChartProps {
  report: CurvesReport;
  mode: DisplayMode;
  measure: CurveMeasure;
  formatValue: (val: number | null | undefined) => string;
  hoveredIdx: number | null;
  onHover: (idx: number | null) => void;
  messages: typeof curvesMessages.vi;
}

function CurvesChart({ report, mode, formatValue, hoveredIdx, onHover, messages: m }: ChartProps) {
  useId();
  const points = report.points;
  const count = points.length;

  const plotW = CHART_WIDTH - CHART_PADDING.left - CHART_PADDING.right;
  const plotH = CHART_HEIGHT - CHART_PADDING.top - CHART_PADDING.bottom;

  // Compute Max Y
  const maxY = useMemo(() => {
    let max = 0;
    if (mode === "cumulative") {
      for (const p of points) {
        max = Math.max(
          max,
          p.scheduled_cumulative,
          p.baseline_cumulative ?? 0,
          p.actual_cumulative ?? 0,
          p.earned_cumulative ?? 0,
        );
      }
    } else {
      for (const p of points) {
        max = Math.max(
          max,
          p.scheduled_period,
          p.baseline_period ?? 0,
          p.actual_period ?? 0,
          p.earned_period ?? 0,
        );
      }
    }
    return max > 0 ? max * 1.08 : 100;
  }, [points, mode]);

  const x = useCallback(
    (idx: number) => CHART_PADDING.left + (count > 1 ? (idx / (count - 1)) * plotW : plotW / 2),
    [count, plotW],
  );

  const y = useCallback(
    (val: number) => CHART_PADDING.top + plotH - (val / maxY) * plotH,
    [maxY, plotH],
  );

  // SVG Paths for S-curves
  const pathFor = useCallback(
    (selector: (p: CurvePoint) => number | null | undefined) => {
      const validPoints: [number, number][] = [];
      points.forEach((p, idx) => {
        const val = selector(p);
        if (val != null) {
          validPoints.push([x(idx), y(val)]);
        }
      });
      if (validPoints.length === 0) return "";
      return validPoints.map(([px, py], i) => `${i === 0 ? "M" : "L"}${px.toFixed(1)},${py.toFixed(1)}`).join(" ");
    },
    [points, x, y],
  );

  // Y-axis ticks
  const yTicks = useMemo(() => {
    const steps = 4;
    return Array.from({ length: steps + 1 }, (_, i) => (maxY / steps) * i);
  }, [maxY]);

  // X-axis ticks (at most 8 labels to avoid overlap)
  const xTickIndices = useMemo(() => {
    if (count <= 8) return points.map((_, i) => i);
    const step = Math.ceil(count / 7);
    const idxs = [];
    for (let i = 0; i < count; i += step) {
      idxs.push(i);
    }
    if (idxs[idxs.length - 1] !== count - 1) {
      idxs.push(count - 1);
    }
    return idxs;
  }, [count, points]);

  // Locate Status Date & Baseline Finish on timeline
  const statusX = useMemo(() => {
    if (report.status_date == null || count === 0) return null;
    const s = typeof report.status_date === "number" ? report.status_date : new Date(report.status_date).getTime();
    const firstTs = typeof points[0].date === "number" ? points[0].date : new Date(points[0].date).getTime();
    const lastTs = typeof points[count - 1].date === "number" ? points[count - 1].date : new Date(points[count - 1].date).getTime();
    if (Number.isNaN(s) || Number.isNaN(firstTs) || Number.isNaN(lastTs)) return null;
    if (s < firstTs || s > lastTs + 86400000 * 30) return null;
    if (lastTs === firstTs) return x(0);
    const ratio = Math.max(0, Math.min(1, (s - firstTs) / (lastTs - firstTs)));
    return CHART_PADDING.left + ratio * plotW;
  }, [report.status_date, count, points, plotW, x]);

  const activePoint = hoveredIdx != null ? points[hoveredIdx] : null;

  return (
    <figure className="curves-figure">
      <svg
        className="curves-svg"
        viewBox={`0 0 ${CHART_WIDTH} ${CHART_HEIGHT}`}
        role="img"
        aria-label={m.chartAria}
        onMouseLeave={() => onHover(null)}
      >
        {/* Background & Grid */}
        <rect
          x={CHART_PADDING.left}
          y={CHART_PADDING.top}
          width={plotW}
          height={plotH}
          style={{ fill: "var(--chart-bg)", stroke: "var(--chart-frame)" }}
        />

        {/* Y Grid & Labels */}
        {yTicks.map((tick) => (
          <g key={`y-${tick}`}>
            <line
              x1={CHART_PADDING.left}
              x2={CHART_PADDING.left + plotW}
              y1={y(tick)}
              y2={y(tick)}
              style={{ stroke: "var(--chart-grid)" }}
            />
            <text
              x={CHART_PADDING.left - 8}
              y={y(tick) + 4}
              textAnchor="end"
              fontSize="10"
              style={{ fill: "var(--text-muted)" }}
            >
              {formatValue(tick)}
            </text>
          </g>
        ))}

        {/* X Axis Labels */}
        {xTickIndices.map((idx) => (
          <text
            key={`x-${idx}`}
            x={x(idx)}
            y={CHART_HEIGHT - CHART_PADDING.bottom + 18}
            textAnchor="middle"
            fontSize="10"
            style={{ fill: "var(--text-muted)" }}
          >
            {points[idx].label}
          </text>
        ))}

        {/* Status Date Line */}
        {statusX != null && (
          <g>
            <line
              x1={statusX}
              x2={statusX}
              y1={CHART_PADDING.top}
              y2={CHART_PADDING.top + plotH}
              style={{ stroke: "var(--chart-status)" }}
              strokeDasharray="4 3"
              strokeWidth={1.5}
            />
            <text
              x={statusX}
              y={CHART_PADDING.top - 6}
              textAnchor="middle"
              fontSize="9"
              style={{ fill: "var(--chart-status)", fontWeight: 600 }}
            >
              {m.statusDate}
            </text>
          </g>
        )}

        {/* Cumulative Mode (S-curves) */}
        {mode === "cumulative" ? (
          <>
            {/* Baseline Planned Line */}
            {report.baseline != null && (
              <path
                d={pathFor((p) => p.baseline_cumulative)}
                fill="none"
                style={{ stroke: "var(--chart-ideal)" }}
                strokeWidth={2}
                strokeDasharray="5 4"
              />
            )}

            {/* Scheduled Line */}
            <path
              d={pathFor((p) => p.scheduled_cumulative)}
              fill="none"
              style={{ stroke: "var(--chart-planned)" }}
              strokeWidth={2}
            />

            {/* Actual Line */}
            <path
              d={pathFor((p) => p.actual_cumulative)}
              fill="none"
              style={{ stroke: "var(--chart-actual)" }}
              strokeWidth={2.5}
            />

            {/* Earned Value Line */}
            {report.measure === "cost" && (
              <path
                d={pathFor((p) => p.earned_cumulative)}
                fill="none"
                style={{ stroke: "var(--chart-ev)" }}
                strokeWidth={2}
              />
            )}

            {/* Data Dots */}
            {points.map((p, idx) => (
              <g key={`dots-${p.label}`}>
                {p.baseline_cumulative != null && (
                  <circle cx={x(idx)} cy={y(p.baseline_cumulative)} r={3} style={{ fill: "var(--chart-ideal)" }} />
                )}
                <circle cx={x(idx)} cy={y(p.scheduled_cumulative)} r={3} style={{ fill: "var(--chart-planned)" }} />
                {p.actual_cumulative != null && (
                  <circle cx={x(idx)} cy={y(p.actual_cumulative)} r={3.5} style={{ fill: "var(--chart-actual)" }} />
                )}
                {p.earned_cumulative != null && (
                  <circle cx={x(idx)} cy={y(p.earned_cumulative)} r={3} style={{ fill: "var(--chart-ev)" }} />
                )}
              </g>
            ))}
          </>
        ) : (
          /* Periodic Mode (Bar chart) */
          points.map((p, idx) => {
            const barW = Math.max(4, Math.min(28, (plotW / count) * 0.28));
            const center = x(idx);
            const schedH = Math.max(0, plotH - (y(p.scheduled_period) - CHART_PADDING.top));
            const baseH = p.baseline_period != null ? Math.max(0, plotH - (y(p.baseline_period) - CHART_PADDING.top)) : 0;
            const actH = p.actual_period != null ? Math.max(0, plotH - (y(p.actual_period) - CHART_PADDING.top)) : 0;

            return (
              <g key={`bars-${p.label}`}>
                {/* Baseline bar */}
                {p.baseline_period != null && (
                  <rect
                    className="curves-bar curves-bar-baseline"
                    x={center - barW * 1.5}
                    y={y(p.baseline_period)}
                    width={barW}
                    height={baseH}
                    style={{ fill: "var(--chart-ideal)", opacity: 0.7 }}
                    rx={1.5}
                  />
                )}
                {/* Scheduled bar */}
                <rect
                  className="curves-bar curves-bar-scheduled"
                  x={center - barW * 0.5}
                  y={y(p.scheduled_period)}
                  width={barW}
                  height={schedH}
                  style={{ fill: "var(--chart-planned)" }}
                  rx={1.5}
                />
                {/* Actual bar */}
                {p.actual_period != null && (
                  <rect
                    className="curves-bar curves-bar-actual"
                    x={center + barW * 0.5}
                    y={y(p.actual_period)}
                    width={barW}
                    height={actH}
                    style={{ fill: "var(--chart-actual)" }}
                    rx={1.5}
                  />
                )}
              </g>
            );
          })
        )}

        {/* Hover interaction columns */}
        {points.map((p, idx) => {
          const colW = plotW / count;
          const leftX = CHART_PADDING.left + idx * colW;
          return (
            <rect
              key={`hover-${p.label}`}
              x={leftX}
              y={CHART_PADDING.top}
              width={colW}
              height={plotH}
              fill="transparent"
              style={{ cursor: "pointer" }}
              onMouseEnter={() => onHover(idx)}
            />
          );
        })}

        {/* Active hover vertical cursor */}
        {hoveredIdx != null && (
          <line
            x1={x(hoveredIdx)}
            x2={x(hoveredIdx)}
            y1={CHART_PADDING.top}
            y2={CHART_PADDING.top + plotH}
            stroke="var(--chart-frame)"
            strokeDasharray="2 2"
          />
        )}
      </svg>

      {/* Legend & Tooltip Bar */}
      <div className="curves-legend">
        {report.baseline != null && (
          <span>
            <i className="swatch ideal" /> {m.seriesBaseline}
          </span>
        )}
        <span>
          <i className="swatch planned" /> {m.seriesScheduled}
        </span>
        <span>
          <i className="swatch actual" /> {m.seriesActual}
        </span>
        {report.measure === "cost" && (
          <span>
            <i className="swatch ev" /> {m.seriesEarned}
          </span>
        )}
        {report.status_date != null && (
          <span>
            <i className="swatch status" /> {m.statusDate}
          </span>
        )}
      </div>

      {/* Point Hover Tooltip */}
      {activePoint && (
        <div className="curves-tooltip" aria-live="polite">
          <strong>{activePoint.label}</strong>
          {mode === "cumulative" ? (
            <div className="tooltip-metrics">
              {activePoint.baseline_cumulative != null && (
                <div>
                  {m.seriesBaseline}: {formatValue(activePoint.baseline_cumulative)}
                </div>
              )}
              <div>
                {m.seriesScheduled}: {formatValue(activePoint.scheduled_cumulative)}
              </div>
              {activePoint.actual_cumulative != null && (
                <div>
                  {m.seriesActual}: {formatValue(activePoint.actual_cumulative)}
                </div>
              )}
              {activePoint.earned_cumulative != null && (
                <div>
                  {m.seriesEarned}: {formatValue(activePoint.earned_cumulative)}
                </div>
              )}
            </div>
          ) : (
            <div className="tooltip-metrics">
              {activePoint.baseline_period != null && (
                <div>
                  {m.seriesBaseline}: {formatValue(activePoint.baseline_period)}
                </div>
              )}
              <div>
                {m.seriesScheduled}: {formatValue(activePoint.scheduled_period)}
              </div>
              {activePoint.actual_period != null && (
                <div>
                  {m.seriesActual}: {formatValue(activePoint.actual_period)}
                </div>
              )}
              {activePoint.earned_period != null && (
                <div>
                  {m.seriesEarned}: {formatValue(activePoint.earned_period)}
                </div>
              )}
            </div>
          )}
        </div>
      )}
    </figure>
  );
}

interface TableProps {
  report: CurvesReport;
  mode: DisplayMode;
  formatValue: (val: number | null | undefined) => string;
  messages: typeof curvesMessages.vi;
}

function CurvesTable({ report, mode, formatValue, messages: m }: TableProps) {
  const isCost = report.measure === "cost";

  return (
    <div className="curves-table-wrapper">
      <h4 className="curves-table-title">{m.tableTitle}</h4>
      <table className="curves-table">
        <thead>
          <tr>
            <th>{m.colPeriod}</th>
            {report.baseline != null && <th>{m.colBaseline}</th>}
            <th>{m.colScheduled}</th>
            <th>{m.colActual}</th>
            {isCost && <th>{m.colEarned}</th>}
          </tr>
        </thead>
        <tbody>
          {report.points.map((p) => {
            const baseVal = mode === "cumulative" ? p.baseline_cumulative : p.baseline_period;
            const schedVal = mode === "cumulative" ? p.scheduled_cumulative : p.scheduled_period;
            const actVal = mode === "cumulative" ? p.actual_cumulative : p.actual_period;
            const earnVal = mode === "cumulative" ? p.earned_cumulative : p.earned_period;

            return (
              <tr key={p.label}>
                <td className="cell-label">{p.label}</td>
                {report.baseline != null && <td className="cell-num">{formatValue(baseVal)}</td>}
                <td className="cell-num">{formatValue(schedVal)}</td>
                <td className="cell-num">{formatValue(actVal)}</td>
                {isCost && <td className="cell-num">{formatValue(earnVal)}</td>}
              </tr>
            );
          })}
        </tbody>
        <tfoot>
          <tr>
            <td>
              <strong>{m.total}</strong>
            </td>
            {report.baseline != null && (
              <td className="cell-num">
                <strong>{formatValue(report.total_baseline)}</strong>
              </td>
            )}
            <td className="cell-num">
              <strong>{formatValue(report.total_scheduled)}</strong>
            </td>
            <td className="cell-num">
              <strong>{formatValue(report.total_actual)}</strong>
            </td>
            {isCost && (
              <td className="cell-num">
                <strong>{formatValue(report.total_earned)}</strong>
              </td>
            )}
          </tr>
        </tfoot>
      </table>
    </div>
  );
}
