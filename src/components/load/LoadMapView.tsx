import { useEffect, useMemo, useRef, useState, type MouseEvent } from "react";

import { errorMessage, loadMap } from "../../api";
import { formatDateKeyDisplay, formatHours, instantToDateKey } from "../../format";
import { getLang, useMessages } from "../../i18n";
import { canvasPalette, useResolvedTheme, type CanvasPalette } from "../../theme";
import type { LoadBucket, LoadCell, LoadGroup, LoadMap, Project } from "../../types";
import { loadMapMessages } from "./LoadMapView.i18n";
import { loadSpanEnd } from "./loadSpan";
import { loadTone, type LoadTone } from "./loadTone";
import { ProBadge } from "../license/ProBadge";

const ROW = 24;
// Wide enough for header size, so date headers never touch.
const CELL = { day: 36, week: 60, month: 72 } as const;
const DEFAULT_WEEKS = 12;
const MAX_PERIODS = { day: 84, week: 52, month: 24 } as const;

type GroupMode = "person" | "role" | "skill";
type ViewMode = "grid" | "histogram";

function formatBucketHeader(key: string, bucket: LoadBucket): string {
  const [year, month, day] = key.split("-");
  const lang = getLang();
  if (bucket === "month") {
    return lang === "en" ? `${month}/${year.slice(2)}` : `T${month}/${year.slice(2)}`;
  }
  return lang === "en" ? `${month}-${day}` : `${day}/${month}`;
}

function longDate(key: string): string {
  return formatDateKeyDisplay(key);
}

/** Hours of a minute count for a tooltip; never empty, so a zero shows as "0". */
function hoursOf(minutes: number): string {
  return formatHours(minutes / 60) || "0";
}

function percent(work: number, capacity: number): string {
  return capacity > 0 ? `${Math.round((work / capacity) * 100)}%` : "";
}

function toneColour(tone: LoadTone, palette: CanvasPalette): string | null {
  switch (tone) {
    case "off":
      return palette.weekend;
    case "light":
      return palette.loadLight;
    case "full":
      return palette.loadFull;
    case "over":
      return palette.loadOver;
    default:
      return null;
  }
}

export function LoadMapView({ project, onPickResource }: { project: Project; onPickResource: (uid: number) => void }) {
  const m = useMessages(loadMapMessages);
  const theme = useResolvedTheme();

  const [groupMode, setGroupMode] = useState<GroupMode>("person");
  const [selectedSkillId, setSelectedSkillId] = useState<number>(() => project.skills[0]?.id ?? 0);
  const [selectedMinLevel, setSelectedMinLevel] = useState<number | undefined>(undefined);

  const [bucket, setBucket] = useState<LoadBucket>("week");
  const [viewMode, setViewMode] = useState<ViewMode>("grid");
  const [baselineNumber, setBaselineNumber] = useState<number | undefined>(undefined);

  const [from, setFrom] = useState(() => instantToDateKey(project.start));
  const [periodCount, setPeriodCount] = useState(DEFAULT_WEEKS);
  const [periodCountText, setPeriodCountText] = useState(String(DEFAULT_WEEKS));

  const [loaded, setLoaded] = useState<{ map: LoadMap; bucket: LoadBucket } | null>(null);
  const [selectedRowIndex, setSelectedRowIndex] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [tip, setTip] = useState("");
  const canvas = useRef<HTMLCanvasElement>(null);

  // Sync skill ID when project skills change
  useEffect(() => {
    if (project.skills.length > 0 && !project.skills.some((s) => s.id === selectedSkillId)) {
      setSelectedSkillId(project.skills[0].id);
    }
  }, [project.skills, selectedSkillId]);

  const maxPeriods = MAX_PERIODS[bucket];

  const to = useMemo(() => loadSpanEnd(from, bucket, periodCount), [from, bucket, periodCount]);

  const groupQuery = useMemo<LoadGroup>(() => {
    if (groupMode === "person") return { type: "person" };
    if (groupMode === "role") return { type: "group" };
    return { type: "skill", id: selectedSkillId, min_level: selectedMinLevel };
  }, [groupMode, selectedSkillId, selectedMinLevel]);

  // Fetch load map on state changes
  useEffect(() => {
    let current = true;
    if (groupMode === "skill" && project.skills.length === 0) {
      setLoaded(null);
      return;
    }
    loadMap(from, to, bucket, groupQuery, baselineNumber)
      .then((next) => {
        if (!current) return;
        setError(null);
        setLoaded({ map: next, bucket });
      })
      .catch((err) => current && setError(errorMessage(err)));
    return () => {
      current = false;
    };
  }, [from, to, bucket, groupQuery, baselineNumber, project]);

  const map = loaded?.map ?? null;
  const shownBucket = loaded?.bucket ?? bucket;
  const width = CELL[shownBucket];

  // Canvas Heatmap rendering
  useEffect(() => {
    if (viewMode !== "grid") return;
    const element = canvas.current;
    const ctx = element?.getContext("2d");
    if (!element || !ctx || !map) return;
    const palette = canvasPalette();
    const ratio = window.devicePixelRatio || 1;
    const cssWidth = map.buckets.length * width;
    const cssHeight = map.rows.length * ROW;
    element.width = cssWidth * ratio;
    element.height = cssHeight * ratio;
    element.style.width = `${cssWidth}px`;
    element.style.height = `${cssHeight}px`;
    ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
    ctx.clearRect(0, 0, cssWidth, cssHeight);
    map.rows.forEach((row, y) => {
      row.cells.forEach((cell, x) => {
        const colour = toneColour(loadTone(cell), palette);
        if (!colour) return;
        ctx.fillStyle = colour;
        ctx.fillRect(x * width + 1, y * ROW + 1, width - 2, ROW - 2);
      });
    });
    ctx.fillStyle = palette.grid;
    for (let y = 1; y < map.rows.length; y++) ctx.fillRect(0, y * ROW, cssWidth, 1);
  }, [map, width, theme, viewMode]);

  function cellAt(event: MouseEvent<HTMLCanvasElement>) {
    if (!map) return null;
    const bounds = event.currentTarget.getBoundingClientRect();
    const x = Math.floor((event.clientX - bounds.left) / width);
    const y = Math.floor((event.clientY - bounds.top) / ROW);
    const row = map.rows[y];
    const cell = row?.cells[x];
    return row && cell ? { row, cell, bucket: map.buckets[x] } : null;
  }

  function hover(event: MouseEvent<HTMLCanvasElement>) {
    const found = cellAt(event);
    if (!found) return setTip("");
    const name = found.row.label || m.noRole;
    const when =
      shownBucket === "week"
        ? m.weekOf(longDate(found.bucket))
        : shownBucket === "month"
          ? m.monthOf(formatBucketHeader(found.bucket, "month"))
          : longDate(found.bucket);
    const totalWork = found.cell.work + (found.cell.unassigned ?? 0);
    setTip(
      m.tipDetail(
        name,
        when,
        hoursOf(found.cell.work),
        hoursOf(found.cell.unassigned ?? 0),
        hoursOf(found.cell.capacity),
        percent(totalWork, found.cell.capacity),
        found.cell.baseline !== undefined ? hoursOf(found.cell.baseline) : undefined,
      ),
    );
  }

  const subject = groupMode === "person" ? m.groupPerson : groupMode === "role" ? m.groupRole : m.groupSkill;

  // Active row for Histogram view
  const activeHistogramRow = map?.rows[selectedRowIndex] ?? map?.rows[0] ?? null;

  return (
    <div className="load-view">
      <div className="load-controls">
        {/* Group Selector */}
        <fieldset className="import-target">
          <legend>{m.groupBy}</legend>
          {(["person", "role", "skill"] as const).map((mode) => (
            <label key={mode}>
              <input
                type="radio"
                name="load-group"
                checked={groupMode === mode}
                onChange={() => {
                  setGroupMode(mode);
                  setSelectedRowIndex(0);
                }}
              />{" "}
              {mode === "person" ? m.groupPerson : mode === "role" ? m.groupRole : m.groupSkill}
              {mode !== "person" && <ProBadge />}
            </label>
          ))}
        </fieldset>

        {/* Skill Specific Filters */}
        {groupMode === "skill" && project.skills.length > 0 && (
          <>
            <label>
              {m.skillSelect}{" "}
              <select value={selectedSkillId} onChange={(e) => {
                  setSelectedSkillId(Number(e.target.value));
                  setSelectedRowIndex(0);
                }}>
                {project.skills.map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.name}
                  </option>
                ))}
              </select>
            </label>
            {(project.skill_levels?.length ?? 0) > 0 && (
              <label>
                {m.minLevel}{" "}
                <select
                  value={selectedMinLevel ?? ""}
                  onChange={(e) => {
                    setSelectedMinLevel(e.target.value ? Number(e.target.value) : undefined);
                    setSelectedRowIndex(0);
                  }}
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

        {/* Period Selector */}
        <fieldset className="import-target">
          <legend>{m.bucket}</legend>
          {(["day", "week", "month"] as const).map((item) => (
            <label key={item}>
              <input
                type="radio"
                name="load-bucket"
                checked={bucket === item}
                onChange={() => {
                  setBucket(item);
                  const def = item === "month" ? 12 : item === "week" ? DEFAULT_WEEKS : 30;
                  setPeriodCount(def);
                  setPeriodCountText(String(def));
                }}
              />{" "}
              {m[item]}
              {item === "month" && <ProBadge />}
            </label>
          ))}
        </fieldset>

        {/* Date & Period Length */}
        <label>
          {m.from} <input type="date" value={from} onChange={(event) => event.target.value && setFrom(event.target.value)} />
        </label>
        <label>
          {bucket === "month" ? m.months : bucket === "week" ? m.weeks : m.days}{" "}
          <input
            type="number"
            min={1}
            max={maxPeriods}
            value={periodCountText}
            onChange={(event) => {
              setPeriodCountText(event.target.value);
              const val = Number(event.target.value);
              if (Number.isInteger(val) && val >= 1 && val <= maxPeriods) setPeriodCount(val);
            }}
            onBlur={() => setPeriodCountText(String(periodCount))}
          />
        </label>

        {/* Baseline Selector */}
        <label>
          {m.baseline}
          <ProBadge />
          <select
            value={baselineNumber ?? ""}
            onChange={(e) => setBaselineNumber(e.target.value === "" ? undefined : Number(e.target.value))}
          >
            <option value="">{m.none}</option>
            {Array.from({ length: 11 }, (_, i) => (
              <option key={i} value={i}>
                {m.baselineOption(i)}
              </option>
            ))}
          </select>
        </label>

        {/* View Mode Toggle: Grid vs Histogram */}
        <fieldset className="import-target">
          <legend>{m.viewMode}</legend>
          <label>
            <input type="radio" name="load-viewmode" checked={viewMode === "grid"} onChange={() => setViewMode("grid")} />{" "}
            {m.modeGrid}
          </label>
          <label>
            <input
              type="radio"
              name="load-viewmode"
              checked={viewMode === "histogram"}
              onChange={() => setViewMode("histogram")}
            />{" "}
            {m.modeHistogram}
            <ProBadge />
          </label>
        </fieldset>

        {viewMode === "grid" && <Legend />}
      </div>

      {error && (
        <p className="bad" role="alert">
          {error}
        </p>
      )}

      {project.resources.length === 0 ? (
        <p className="meta">{m.nobody}</p>
      ) : groupMode === "skill" && project.skills.length === 0 ? (
        <p className="meta">{m.noSkills}</p>
      ) : !map ? (
        <p className="meta">{m.loading}</p>
      ) : viewMode === "grid" ? (
        <div className="load-scroll">
          <div className="load-grid" style={{ gridTemplateColumns: `max-content ${map.buckets.length * width}px` }}>
            <div className="load-corner">{subject}</div>
            <div className="load-head" role="row">
              {map.buckets.map((key) => (
                <span key={key} role="columnheader" style={{ width }}>
                  {formatBucketHeader(key, shownBucket)}
                </span>
              ))}
            </div>
            <div className="load-names">
              {map.rows.map((row) => {
                const label = row.label || m.noRole;
                return row.resource_uid !== undefined ? (
                  <button key={row.label + row.resource_uid} type="button" title={m.pick(label)} onClick={() => onPickResource(row.resource_uid!)}>
                    {label}
                  </button>
                ) : (
                  <span key={row.label} className="load-group-label">
                    {label}
                  </span>
                );
              })}
            </div>
            <canvas
              ref={canvas}
              role="img"
              aria-label={m.grid(map.rows.length, map.buckets.length)}
              title={tip}
              onMouseMove={hover}
              onMouseLeave={() => setTip("")}
              onClick={(event) => {
                const found = cellAt(event);
                if (found && found.row.resource_uid !== undefined) onPickResource(found.row.resource_uid);
              }}
            />
          </div>
        </div>
      ) : (
        /* Histogram Mode */
        <div className="load-histogram-container">
          {map.rows.length > 1 && (
            <div className="load-histogram-selector">
              <label>
                {subject}:{" "}
                <select value={selectedRowIndex} onChange={(e) => setSelectedRowIndex(Number(e.target.value))}>
                  {map.rows.map((r, i) => (
                    <option key={i} value={i}>
                      {r.label || m.noRole}
                    </option>
                  ))}
                </select>
              </label>
            </div>
          )}

          {activeHistogramRow && (
            <HistogramChart
              row={activeHistogramRow}
              buckets={map.buckets}
              bucketKind={shownBucket}
              baselineNumber={baselineNumber}
            />
          )}
        </div>
      )}
    </div>
  );
}

function Legend() {
  const m = useMessages(loadMapMessages);
  const tones: LoadTone[] = ["free", "light", "full", "over", "off"];
  return (
    <ul className="load-legend" aria-label={m.legend}>
      {tones.map((tone) => (
        <li key={tone}>
          <span className={`load-swatch ${tone}`} aria-hidden="true" />
          {m[tone]}
        </li>
      ))}
    </ul>
  );
}

/**
 * Histogram Loading Chart:
 * Per period: a stacked bar of demand (assigned + unassigned) against capacity line,
 * with overloaded bar markings and optional baseline ticks.
 */
function HistogramChart({
  row,
  buckets,
  bucketKind,
  baselineNumber,
}: {
  row: { label: string; cells: LoadCell[] };
  buckets: string[];
  bucketKind: LoadBucket;
  baselineNumber?: number;
}) {
  const m = useMessages(loadMapMessages);
  const [hoverIndex, setHoverIndex] = useState<number | null>(null);

  // Determine max height scale
  const maxVal = useMemo(() => {
    let max = 0;
    for (const cell of row.cells) {
      const demand = (cell.work + (cell.unassigned ?? 0)) / 60;
      const cap = cell.capacity / 60;
      const base = (cell.baseline ?? 0) / 60;
      max = Math.max(max, demand, cap, base);
    }
    return Math.max(max * 1.15, 8); // at least 8 hours scale
  }, [row.cells]);

  const height = 240;
  const paddingLeft = 45;
  const paddingBottom = 40;
  const paddingTop = 20;
  const colWidth = Math.max(CELL[bucketKind], 48);
  const chartWidth = Math.max(buckets.length * colWidth, 400);
  const plotHeight = height - paddingTop - paddingBottom;

  const hovered = hoverIndex !== null ? row.cells[hoverIndex] : undefined;
  const yScale = (hours: number) => plotHeight - (hours / maxVal) * plotHeight;

  return (
    <div className="load-histogram-box">
      <div className="load-histogram-header">
        <strong>{row.label || m.noRole}</strong>
        <div className="load-histogram-legend">
          <span><span className="swatch assigned" /> {m.assigned}</span>
          <span><span className="swatch unassigned" /> {m.unassigned}</span>
          <span><span className="swatch capacity-line" /> {m.capacityLabel}</span>
          {baselineNumber !== undefined && (
            <span><span className="swatch baseline-tick" /> {m.baselineLabel} {baselineNumber}</span>
          )}
          <span><span className="swatch overloaded" /> {m.over}</span>
        </div>
      </div>

      <div className="load-histogram-scroll">
        <svg
          width={chartWidth + paddingLeft + 20}
          height={height}
          className="load-histogram-svg"
          onMouseLeave={() => setHoverIndex(null)}
        >
          {/* Y Axis Grid & Labels */}
          {[0, 0.25, 0.5, 0.75, 1].map((pct) => {
            const val = Math.round(maxVal * pct);
            const y = paddingTop + yScale(val);
            return (
              <g key={pct}>
                <line x1={paddingLeft} y1={y} x2={paddingLeft + chartWidth} y2={y} strokeDasharray="2,2" style={{ stroke: "var(--chart-grid)" }} />
                <text x={paddingLeft - 6} y={y + 4} textAnchor="end" className="load-axis-label">
                  {m.axisHours(formatHours(val) || "0")}
                </text>
              </g>
            );
          })}

          {/* Bars & Ticks */}
          {row.cells.map((cell, i) => {
            const x = paddingLeft + i * colWidth;
            const barW = colWidth - 12;
            const assignedH = ((cell.work / 60) / maxVal) * plotHeight;
            const unassignedH = (((cell.unassigned ?? 0) / 60) / maxVal) * plotHeight;
            const totalH = assignedH + unassignedH;
            const capY = paddingTop + yScale(cell.capacity / 60);
            const isOverloaded = (cell.work + (cell.unassigned ?? 0)) > cell.capacity;
            const baseY = cell.baseline !== undefined ? paddingTop + yScale(cell.baseline / 60) : null;

            return (
              <g
                key={i}
                onMouseEnter={() => setHoverIndex(i)}
                style={{ cursor: "pointer" }}
              >
                {/* Background column highlight on hover */}
                {hoverIndex === i && (
                  <rect
                    x={x}
                    y={paddingTop}
                    width={colWidth}
                    height={plotHeight}
                    opacity={0.4}
                    style={{ fill: "var(--surface-sunken)" }}
                  />
                )}

                {/* Assigned Demand Bar */}
                <rect
                  x={x + 6}
                  y={paddingTop + plotHeight - assignedH}
                  width={barW}
                  height={Math.max(0, assignedH)}
                  className={`bar-assigned ${isOverloaded ? "overloaded" : ""}`}
                />

                {/* Unassigned Demand Bar (Stacked on top) */}
                {unassignedH > 0 && (
                  <rect
                    x={x + 6}
                    y={paddingTop + plotHeight - totalH}
                    width={barW}
                    height={Math.max(0, unassignedH)}
                    className="bar-unassigned"
                  />
                )}

                {/* Overload indicator border if total demand > capacity */}
                {isOverloaded && (
                  <rect
                    x={x + 5}
                    y={paddingTop + plotHeight - totalH - 1}
                    width={barW + 2}
                    height={Math.max(0, totalH + 2)}
                    strokeWidth={2}
                    rx={2}
                    style={{ fill: "none", stroke: "var(--load-over)" }}
                  />
                )}

                {/* Baseline Tick */}
                {baseY !== null && (
                  <line
                    x1={x + 2}
                    y1={baseY}
                    x2={x + colWidth - 2}
                    y2={baseY}
                    strokeWidth={2.5}
                    strokeDasharray="4,2"
                    style={{ stroke: "var(--chart-ideal)" }}
                  />
                )}

                {/* Capacity Tick / Line segment */}
                <line
                  x1={x + 4}
                  y1={capY}
                  x2={x + colWidth - 4}
                  y2={capY}
                  strokeWidth={2}
                  style={{ stroke: "var(--text-muted)" }}
                />

                {/* X Axis Label */}
                <text
                  x={x + colWidth / 2}
                  y={height - 12}
                  textAnchor="middle"
                  className="load-axis-label"
                >
                  {formatBucketHeader(buckets[i], bucketKind)}
                </text>
              </g>
            );
          })}

          {/* Capacity Connecting Line */}
          <path
            d={row.cells
              .map((cell, i) => {
                const x = paddingLeft + i * colWidth + colWidth / 2;
                const y = paddingTop + yScale(cell.capacity / 60);
                return `${i === 0 ? "M" : "L"} ${x} ${y}`;
              })
              .join(" ")}
            strokeWidth={1.5}
            style={{ fill: "none", stroke: "var(--text-muted)" }}
            strokeDasharray="3,3"
            opacity={0.7}
          />
        </svg>

        {/* Hover Tooltip Popup */}
        {hovered && (
          <div className="load-histogram-tooltip">
            <div className="tip-header">
              {formatBucketHeader(buckets[hoverIndex!], bucketKind)} ({formatDateKeyDisplay(buckets[hoverIndex!])})
            </div>
            <div>
              {m.assigned}: <strong>{m.hoursValue(hoursOf(hovered.work))}</strong>
            </div>
            {(hovered.unassigned ?? 0) > 0 && (
              <div>
                {m.unassigned}: <strong>{m.hoursValue(hoursOf(hovered.unassigned ?? 0))}</strong>
              </div>
            )}
            <div>
              {m.capacityLabel}: <strong>{m.hoursValue(hoursOf(hovered.capacity))}</strong>
            </div>
            <div>
              {m.loadLabel}: <strong>{percent(hovered.work + (hovered.unassigned ?? 0), hovered.capacity)}</strong>
            </div>
            {hovered.baseline !== undefined && (
              <div>
                {m.baselineLabel}: <strong>{m.hoursValue(hoursOf(hovered.baseline))}</strong>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
