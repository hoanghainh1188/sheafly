import { useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties, type RefObject } from "react";

import { shiftDays } from "../format";
import { resourceNamesByTask } from "../resourceNames";
import {
  ROW,
  barBox,
  barColor,
  buildTimescale,
  dayLabelParts,
  resourceLabelX,
  holidayKeys,
  isWeekend,
  isoDate,
  levelingGapAt,
  levelingGaps,
  pieceSpan,
  previewPieces,
  segmentAt,
  segmentPreview,
  progressWidths,
  splitPieces,
  timescaleBands,
  type Timescale,
} from "../ganttLayout";
import { ganttKeyAction } from "../ganttKeys";
import { useLang, useMessages } from "../i18n";
import { canvasPalette, useResolvedTheme, type CanvasPalette } from "../theme";
import { anchoredScroll, wheelPixels, wheelZoom, zoomedPx } from "../ganttZoom";
import { UI_FONT_FAMILY } from "../uiFont";
import type { Project, ProjectDiffReport, Scale, Segment, Task, TaskDiff } from "../types";
import type { ViewRow } from "../viewRows";
import { ganttChartMessages } from "./GanttChart.i18n";

interface DragState {
  kind: "move" | "resize" | "link" | "segment";
  uid: number;
  /** For "segment": which piece of the split bar is dragged (1 or later). */
  segment?: number;
  row: number;
  originX: number;
  originY: number;
  start: number;
  finish: number;
  x: number;
  y: number;
  moved: boolean;
}

interface Props {
  project: Project;
  rows: ViewRow[];
  scale: Scale;
  /** How much wider than the scale's own a day column is (Cmd/Ctrl + wheel, pinch, menu). */
  zoom?: number;
  /** A new zoom from Cmd/Ctrl + wheel or a pinch; the chart keeps the date under the pointer. */
  onZoom?: (zoom: number) => void;
  selected: number | null;
  tracking: boolean;
  bodyRef: RefObject<HTMLDivElement | null>;
  onSelect: (uid: number) => void;
  /** The end of the Shift range, or null when only the selected row is. */
  selectionEnd?: number | null;
  /** Shift+arrow keys moved the end of the range to `uid`. */
  onExtend?: (uid: number) => void;
  onMove: (uid: number, start: number) => void;
  onResize: (uid: number, finish: number) => void;
  /** Drag piece `index` (1 or later) of a split bar to start at `start`. */
  onMoveSegment: (uid: number, index: number, start: number) => void;
  onLink: (from: number, to: number) => void;
  /** Enter on the focused chart opens the selected task. */
  onEdit: (uid: number) => void;
  onScroll: () => void;
  diffReport?: ProjectDiffReport | null;
}

export function GanttChart({
  project,
  rows,
  scale,
  zoom = 1,
  onZoom,
  selected,
  tracking,
  bodyRef,
  onSelect,
  selectionEnd,
  onExtend,
  onMove,
  onResize,
  onMoveSegment,
  onLink,
  onEdit,
  onScroll,
  diffReport,
}: Props) {
  const diffTaskMap = useMemo(() => {
    if (!diffReport) return null;
    const map = new Map<number, TaskDiff>();
    for (const t of diffReport.tasks) {
      map.set(t.uid, t);
    }
    return map;
  }, [diffReport]);
  const headRef = useRef<HTMLDivElement>(null);
  const bodyCanvas = useRef<HTMLCanvasElement>(null);
  const [drag, setDrag] = useState<DragState | null>(null);
  /** Tooltip of the canvas: set while the pointer is over a gap leveling made. */
  const [tip, setTip] = useState("");
  const px = zoomedPx(scale, zoom);
  // Where the last zoom was aimed, so the date under the pointer stays put.
  const anchor = useRef<{ x: number } | null>(null);
  const shownPx = useRef(px);
  const zoomState = useRef({ scale, zoom, onZoom });
  zoomState.current = { scale, zoom, onZoom };
  const pending = useRef<number | null>(null);
  pending.current = null;

  useEffect(() => {
    const scroller = bodyRef.current;
    if (!scroller) return;
    // A native listener: React's wheel listener is passive and cannot stop the scroll.
    const wheel = (event: WheelEvent) => {
      const { scale: s, zoom: z, onZoom: change } = zoomState.current;
      if (!(event.ctrlKey || event.metaKey) || !change) return;
      event.preventDefault();
      // Several events can arrive before the next render: build on the last one sent.
      const from = pending.current ?? z;
      const next = wheelZoom(s, from, wheelPixels(event));
      if (next === from) return;
      pending.current = next;
      anchor.current = { x: event.clientX - scroller.getBoundingClientRect().left };
      change(next);
    };
    scroller.addEventListener("wheel", wheel, { passive: false });
    return () => scroller.removeEventListener("wheel", wheel);
  }, [bodyRef]);

  // Keep the anchored date in place once the wider or narrower columns are laid out; a
  // zoom from the menu or keys anchors at the middle of the view.
  useLayoutEffect(() => {
    const scroller = bodyRef.current;
    const before = shownPx.current;
    shownPx.current = px;
    if (!scroller || before === px) {
      anchor.current = null;
      return;
    }
    const x = anchor.current?.x ?? scroller.clientWidth / 2;
    anchor.current = null;
    scroller.scrollLeft = anchoredScroll(scroller.scrollLeft, x, before, px);
    // The header follows in the same frame, not one scroll event later.
    if (headRef.current) headRef.current.scrollLeft = scroller.scrollLeft;
  }, [px, bodyRef]);
  const model = useMemo(() => buildTimescale(project), [project]);
  const m = useMessages(ganttChartMessages);
  // Band labels are in the interface language, so they are rebuilt when it changes.
  const lang = useLang();
  const bands = useMemo(() => timescaleBands(model.days, px), [model.days, px, lang]);
  const timelineWidth = model.days.length * px;
  const names = useMemo(
    () => resourceNamesByTask(project.assignments, project.resources),
    [project.assignments, project.resources],
  );
  // Colours come from the CSS tokens of the theme on screen; read again when it changes.
  const theme = useResolvedTheme();
  const palette = useMemo(() => canvasPalette(), [theme]);

  function paint() {
    const canvas = bodyCanvas.current;
    const scroller = bodyRef.current;
    if (!canvas || !scroller) return;
    paintBody(
      canvas,
      project,
      names,
      rows,
      model,
      px,
      selected,
      tracking,
      drag,
      scroller.scrollTop,
      scroller.scrollLeft,
      scroller.clientHeight,
      scroller.clientWidth,
      palette,
      diffTaskMap,
    );
  }

  useEffect(() => {
    paint();
    const scroller = bodyRef.current;
    if (!scroller) return;
    const observer = new ResizeObserver(() => paint());
    observer.observe(scroller);
    return () => observer.disconnect();
  }, [project, names, rows, model, px, selected, tracking, drag, palette]);

  function contentPoint(event: { clientX: number; clientY: number; currentTarget: HTMLCanvasElement }) {
    const bounds = event.currentTarget.getBoundingClientRect();
    return {
      // The canvas sits at the scrolled-to part of the timeline (`paintBody`).
      x: event.clientX - bounds.left + (Number.parseFloat(event.currentTarget.style.left) || 0),
      y: event.clientY - bounds.top + (bodyRef.current?.scrollTop ?? 0),
    };
  }

  function hit(x: number, y: number) {
    const row = Math.floor(y / ROW);
    const item = rows[row];
    if (!item || item.kind !== "task") return null;
    const task = item.task;
    const rect = barBox(task, row, model.origin, px);
    const handle = { x: rect.x + Math.max(rect.w, 8), y: rect.y + rect.h / 2 };
    if (Math.hypot(x - handle.x, y - handle.y) <= 7) return { task, row, zone: "link" as const, rect };
    if (!task.summary && !task.milestone && x >= rect.x + rect.w - 6 && x <= rect.x + rect.w + 2 && y >= rect.y && y <= rect.y + rect.h) {
      return { task, row, zone: "resize" as const, rect };
    }
    if (x >= rect.x - 4 && x <= rect.x + Math.max(rect.w, 10) && y >= rect.y - 2 && y <= rect.y + rect.h + 2) {
      return { task, row, zone: "body" as const, rect };
    }
    return { task, row, zone: "row" as const, rect };
  }

  return (
    <div className="gantt">
      <div className="gantt-head" ref={headRef}>
        <div className="gantt-scale" style={{ width: timelineWidth, minWidth: timelineWidth }}>
          <div className="gantt-bands">
            {bands.map((band, index) => (
              <div key={`${band.label}-${index}`} className="gantt-band" style={fixedSpan(band.count * px)}>
                {band.label}
              </div>
            ))}
          </div>
          <div className="gantt-days">
            {model.days.map((day) => (
              <DayCell key={day} day={day} px={px} />
            ))}
          </div>
        </div>
      </div>
      <div
        className="gantt-body"
        ref={bodyRef}
        tabIndex={0}
        role="group"
        aria-label={m.chart}
        onKeyDown={(event) => {
          // A held key would send the same edit again before the new dates arrive.
          if (event.repeat && event.altKey) return;
          const action = ganttKeyAction(event, rows, selected, project, selectionEnd ?? null);
          if (!action) return;
          event.preventDefault();
          event.stopPropagation();
          if (action.kind === "select" || action.kind === "extend") {
            if (action.kind === "select") onSelect(action.uid);
            else onExtend?.(action.uid);
            revealRow(event.currentTarget, rows, action.uid);
          } else if (action.kind === "move") onMove(action.uid, action.start);
          else if (action.kind === "resize") onResize(action.uid, action.finish);
          else onEdit(action.uid);
        }}
        onScroll={(event) => {
          if (headRef.current) headRef.current.scrollLeft = event.currentTarget.scrollLeft;
          const canvas = bodyCanvas.current;
          if (canvas) {
            paintBody(
              canvas,
              project,
              names,
              rows,
              model,
              px,
              selected,
              tracking,
              drag,
              event.currentTarget.scrollTop,
              event.currentTarget.scrollLeft,
              event.currentTarget.clientHeight,
              event.currentTarget.clientWidth,
              palette,
              diffTaskMap,
            );
          }
          onScroll();
        }}
      >
        <div
          className="gantt-canvas-host"
          style={{
            width: timelineWidth,
            minWidth: timelineWidth,
            height: rows.length * ROW,
          }}
        >
        <canvas
          ref={bodyCanvas}
          onPointerDown={(event) => {
            const { x, y } = contentPoint(event);
            const found = hit(x, y);
            if (!found) return;
            setTip("");
            onSelect(found.task.uid);
            if (found.zone === "row") return;
            event.currentTarget.setPointerCapture(event.pointerId);
            // The first piece of a split bar moves the whole task; a later one moves on its own.
            const piece = found.zone === "body" ? segmentAt(found.task, x, model.origin, px) : null;
            const segment = piece != null && piece > 0 ? piece : undefined;
            setDrag({
              kind:
                found.zone === "link" ? "link" : found.zone === "resize" ? "resize" : segment != null ? "segment" : "move",
              uid: found.task.uid,
              segment,
              row: found.row,
              originX: x,
              originY: y,
              start: segment != null ? found.task.segments[segment].start : found.task.start,
              finish: found.task.finish,
              x,
              y,
              moved: false,
            });
          }}
          title={tip}
          onPointerLeave={() => setTip("")}
          onPointerMove={(event) => {
            const { x, y } = contentPoint(event);
            if (!drag) {
              const found = hit(x, y);
              const over = found != null && levelingGapAt(found.task, x, model.origin, px);
              setTip(over ? m.levelingGap : "");
              return;
            }
            setDrag({ ...drag, x, y, moved: drag.moved || Math.hypot(x - drag.originX, y - drag.originY) > 4 });
          }}
          onPointerUp={(event) => {
            if (!drag) return;
            const { x, y } = contentPoint(event);
            if (drag.moved) {
              const days = Math.round((x - drag.originX) / px);
              if (drag.kind === "move" && days !== 0) onMove(drag.uid, shiftDays(drag.start, days));
              if (drag.kind === "resize" && days !== 0) onResize(drag.uid, shiftDays(drag.finish, days));
              if (drag.kind === "segment" && drag.segment != null && days !== 0) {
                onMoveSegment(drag.uid, drag.segment, shiftDays(drag.start, days));
              }
              if (drag.kind === "link") {
                const found = hit(x, y);
                if (found && found.task.uid !== drag.uid) onLink(drag.uid, found.task.uid);
              }
            }
            setDrag(null);
          }}
        />
        </div>
      </div>
    </div>
  );
}

function fixedSpan(width: number): CSSProperties {
  // flex-shrink 0 keeps later weeks and months the same width as the current period.
  return { width, minWidth: width, maxWidth: width, flex: `0 0 ${width}px` };
}

function paintBody(
  canvas: HTMLCanvasElement,
  project: Project,
  names: Map<number, string>,
  rows: ViewRow[],
  model: Timescale,
  px: number,
  selected: number | null,
  tracking: boolean,
  drag: DragState | null,
  scrollTop: number,
  scrollLeft: number,
  viewHeight: number,
  viewWidth: number,
  palette: CanvasPalette,
  diffTaskMap: Map<number, TaskDiff> | null = null,
) {
  const width = model.days.length * px;
  // The canvas covers only the part of the timeline in view, so it stays far below the
  // size a browser will draw however long the project or wide the zoom.
  const canvasWidth = Math.max(1, Math.min(width, viewWidth > 0 ? viewWidth : width));
  const canvasLeft = Math.max(0, Math.min(scrollLeft, width - canvasWidth));
  const contentHeight = Math.max(rows.length * ROW, 1);
  const canvasHeight = Math.max(1, Math.min(Math.max(viewHeight, ROW), contentHeight));
  const ratio = window.devicePixelRatio || 1;
  canvas.style.top = `${scrollTop}px`;
  canvas.style.left = `${canvasLeft}px`;
  sizeCanvas(canvas, canvasWidth, canvasHeight, ratio);
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  ctx.setTransform(ratio, 0, 0, ratio, -canvasLeft * ratio, -scrollTop * ratio);
  ctx.clearRect(canvasLeft, scrollTop, canvasWidth, canvasHeight);
  const windowHeight = Math.max(viewHeight, canvasHeight);
  const rowStart = Math.max(0, Math.floor(scrollTop / ROW) - 6);
  const rowEnd = Math.min(rows.length, rowStart + Math.ceil(windowHeight / ROW) + 12);
  const holidays = holidayKeys(project);
  const labels: { text: string; left: number; right: number; y: number }[] = [];
  model.days.forEach((day, index) => {
    const key = isoDate(day);
    if (isWeekend(day) || holidays.has(key)) {
      ctx.fillStyle = palette.weekend;
      ctx.fillRect(index * px, scrollTop, px, canvasHeight);
    }
    ctx.strokeStyle = palette.grid;
    ctx.beginPath();
    ctx.moveTo(index * px + 0.5, scrollTop);
    ctx.lineTo(index * px + 0.5, scrollTop + canvasHeight);
    ctx.stroke();
  });
  for (let row = rowStart; row < rowEnd; row += 1) {
    const item = rows[row];
    const y = row * ROW;
    if (item.kind === "group") {
      ctx.fillStyle = palette.group;
      ctx.fillRect(0, y, width, ROW);
      ctx.fillStyle = palette.text;
      ctx.font = `600 12px ${UI_FONT_FAMILY}`;
      ctx.textAlign = "left";
      ctx.textBaseline = "middle";
      ctx.fillText(item.label, scrollLeft + 8, y + ROW / 2);
      ctx.strokeStyle = palette.groupRule;
      ctx.beginPath();
      ctx.moveTo(0, y + ROW - 0.5);
      ctx.lineTo(width, y + ROW - 0.5);
      ctx.stroke();
      continue;
    }
    const task = item.task;
    if (item.highlighted) {
      // Amber wash stays lighter than the critical-path bar so both remain readable.
      ctx.fillStyle = palette.highlight;
      ctx.fillRect(0, y, width, ROW);
    }
    if (task.uid === selected) {
      ctx.fillStyle = palette.selected;
      ctx.fillRect(0, y, width, ROW);
    }
    ctx.strokeStyle = palette.gridRow;
    ctx.beginPath();
    ctx.moveTo(0, y + ROW - 0.5);
    ctx.lineTo(width, y + ROW - 0.5);
    ctx.stroke();
    if (tracking) {
      const baseline = task.baselines.find((baselineItem) => baselineItem.number === 0);
      if (baseline) {
        const pieces = splitPieces(task, baseline.segments);
        drawBar(ctx, task, row, model.origin, px, palette.baseline, palette.knob, true, baseline.start, baseline.finish, pieces);
      }
    }
    if (diffTaskMap) {
      const diffTask = diffTaskMap.get(task.uid);
      if (diffTask && (diffTask.kind === "Modified" || diffTask.kind === "Removed") && diffTask.old_start && diffTask.old_finish) {
        drawBar(ctx, task, row, model.origin, px, "rgba(148, 163, 184, 0.55)", palette.knob, true, diffTask.old_start, diffTask.old_finish, []);
      }
    }
    const moved = drag && drag.moved && drag.uid === task.uid && drag.kind !== "link" ? drag : null;
    const start = moved?.kind === "move" ? shiftDays(task.start, Math.round((moved.x - moved.originX) / px)) : task.start;
    const finish = moved?.kind === "resize" ? shiftDays(task.finish, Math.round((moved.x - moved.originX) / px)) : task.finish;
    const pieces =
      moved?.kind === "segment" && moved.segment != null
        ? segmentPreview(task, moved.segment, shiftDays(moved.start, Math.round((moved.x - moved.originX) / px)))
        : previewPieces(task, start, finish);
    const preview = { ...task, start, finish: moved?.kind === "segment" && pieces ? pieces[pieces.length - 1].finish : finish };
    const color = barColor(task, palette);
    drawBar(ctx, preview, row, model.origin, px, color, palette.knob, false, preview.start, preview.finish, pieces, palette.levelingGap);
    if (!task.summary && task.percent_complete > 0) {
      const rect = barBox(preview, row, model.origin, px);
      ctx.fillStyle = palette.progress;
      if (pieces) {
        const spans = pieces.map((piece) => pieceSpan(piece, model.origin, px));
        progressWidths(spans, task.percent_complete).forEach((done, index) => {
          if (done > 0) ctx.fillRect(spans[index].x, rect.y, done, rect.h);
        });
      } else {
        ctx.fillRect(rect.x, rect.y, rect.w * Math.min(task.percent_complete, 100) / 100, rect.h);
      }
    }
    const label = task.summary ? "" : names.get(task.uid);
    if (label) {
      const rect = barBox(preview, row, model.origin, px);
      labels.push({ text: label, left: rect.x, right: rect.x + Math.max(rect.w, rect.h), y: y + ROW / 2 });
    }
  }
  const indexOf = new Map<number, number>();
  rows.forEach((item, index) => {
    if (item.kind === "task" && !indexOf.has(item.task.uid)) indexOf.set(item.task.uid, index);
  });
  const viewTop = scrollTop;
  const viewBottom = scrollTop + canvasHeight;
  ctx.lineWidth = 1.2;
  for (const dep of project.dependencies) {
    const from = indexOf.get(dep.predecessor_uid);
    const to = indexOf.get(dep.successor_uid);
    if (from === undefined || to === undefined) continue;
    const linkTop = Math.min(from, to) * ROW;
    const linkBottom = Math.max(from, to) * ROW + ROW;
    if (linkBottom < viewTop || linkTop > viewBottom) continue;
    const predRow = rows[from];
    const succRow = rows[to];
    if (predRow.kind !== "task" || succRow.kind !== "task") continue;
    const pred = predRow.task;
    const succ = succRow.task;
    const a = barBox(pred, from, model.origin, px);
    const b = barBox(succ, to, model.origin, px);
    const startX = dep.kind === "ss" || dep.kind === "sf" ? a.x : a.x + a.w;
    const startY = a.y + a.h / 2;
    const endX = dep.kind === "ff" || dep.kind === "sf" ? b.x + b.w : b.x;
    const endY = b.y + b.h / 2;
    ctx.strokeStyle = pred.critical && succ.critical ? palette.critical : palette.link;
    ctx.beginPath();
    ctx.moveTo(startX, startY);
    const mid = startX + (endX >= startX ? 10 : -10);
    ctx.lineTo(mid, startY);
    ctx.lineTo(mid, endY);
    ctx.lineTo(endX, endY);
    ctx.stroke();
    ctx.beginPath();
    ctx.moveTo(endX, endY);
    ctx.lineTo(endX - 5, endY - 3);
    ctx.lineTo(endX - 5, endY + 3);
    ctx.closePath();
    ctx.fillStyle = ctx.strokeStyle;
    ctx.fill();
  }
  drawResourceLabels(ctx, labels, width, palette);
  if (drag?.kind === "link" && drag.moved) {
    const from = rows[drag.row];
    if (from?.kind === "task") {
      const rect = barBox(from.task, drag.row, model.origin, px);
      ctx.strokeStyle = palette.accent;
      ctx.setLineDash([4, 3]);
      ctx.beginPath();
      ctx.moveTo(rect.x + rect.w, rect.y + rect.h / 2);
      ctx.lineTo(drag.x, drag.y);
      ctx.stroke();
      ctx.setLineDash([]);
    }
  }
}

function drawBar(
  ctx: CanvasRenderingContext2D,
  task: Task,
  row: number,
  origin: number,
  px: number,
  color: string,
  knob: string,
  baseline: boolean,
  start: number,
  finish: number,
  pieces: Segment[] | null,
  /** Colour of the gaps leveling made; they are drawn as dashed boxes. Unset for a baseline. */
  levelingGap?: string,
) {
  const shaped = { ...task, start, finish };
  const rect = barBox(shaped, row, origin, px);
  const y = baseline ? row * ROW + 20 : rect.y;
  const h = baseline ? 5 : rect.h;
  ctx.fillStyle = color;
  if (pieces) {
    // Split task: one block per segment, joined by a dotted line across each gap.
    const spans = pieces.map((piece) => pieceSpan(piece, origin, px));
    spans.forEach((span) => ctx.fillRect(span.x, y, span.w, h));
    ctx.save();
    ctx.strokeStyle = color;
    ctx.lineWidth = 1.2;
    ctx.setLineDash([2, 3]);
    const marks = levelingGap ? levelingGaps(task) : [];
    for (let index = 0; index + 1 < spans.length; index += 1) {
      const from = spans[index].x + spans[index].w;
      if (marks[index] && levelingGap) {
        // A pause leveling made for an urgent task: a dashed box, apart from the user's dotted line.
        ctx.strokeStyle = levelingGap;
        ctx.setLineDash([4, 3]);
        ctx.strokeRect(from + 0.5, y + 0.5, Math.max(spans[index + 1].x - from - 1, 1), h - 1);
        ctx.strokeStyle = color;
        ctx.setLineDash([2, 3]);
        continue;
      }
      ctx.beginPath();
      ctx.moveTo(from, y + h / 2);
      ctx.lineTo(spans[index + 1].x, y + h / 2);
      ctx.stroke();
    }
    ctx.setLineDash([]);
    if (!baseline) {
      const last = spans[spans.length - 1];
      ctx.fillStyle = knob;
      ctx.beginPath();
      ctx.arc(last.x + last.w, y + h / 2, 3.5, 0, Math.PI * 2);
      ctx.fill();
      ctx.stroke();
    }
    ctx.restore();
    return;
  }
  if (task.milestone && !baseline) {
    const size = 7;
    const cx = rect.x;
    const cy = row * ROW + ROW / 2;
    ctx.beginPath();
    ctx.moveTo(cx, cy - size);
    ctx.lineTo(cx + size, cy);
    ctx.lineTo(cx, cy + size);
    ctx.lineTo(cx - size, cy);
    ctx.closePath();
    ctx.fill();
    ctx.fillStyle = knob;
    ctx.beginPath();
    ctx.arc(cx + 11, cy, 3.5, 0, Math.PI * 2);
    ctx.fill();
    ctx.strokeStyle = color;
    ctx.stroke();
    return;
  }
  ctx.fillRect(rect.x, y, Math.max(rect.w, 2), h);
  if (task.summary && !baseline) {
    ctx.beginPath();
    ctx.moveTo(rect.x, y);
    ctx.lineTo(rect.x + 7, y);
    ctx.lineTo(rect.x, y + h);
    ctx.moveTo(rect.x + rect.w, y);
    ctx.lineTo(rect.x + rect.w - 7, y);
    ctx.lineTo(rect.x + rect.w, y + h);
    ctx.fill();
  } else if (!baseline && !task.summary) {
    ctx.fillStyle = knob;
    ctx.beginPath();
    ctx.arc(rect.x + rect.w, y + h / 2, 3.5, 0, Math.PI * 2);
    ctx.fill();
    ctx.strokeStyle = color;
    ctx.stroke();
  }
}

function sizeCanvas(canvas: HTMLCanvasElement, width: number, height: number, ratio: number) {
  canvas.style.width = `${width}px`;
  canvas.style.height = `${height}px`;
  canvas.style.minWidth = `${width}px`;
  canvas.style.maxWidth = "none";
  canvas.style.flexShrink = "0";
  canvas.width = Math.max(1, Math.floor(width * ratio));
  canvas.height = Math.max(1, Math.floor(height * ratio));
}


function DayCell({ day, px }: { day: number; px: number }) {
  const { weekday, date } = dayLabelParts(day, px);
  return (
    <div className={`gantt-day${isWeekend(day) ? " weekend" : ""}`} style={fixedSpan(px)}>
      {weekday && <span className="gantt-day-weekday">{weekday}</span>}
      <span className="gantt-day-date">{date}</span>
    </div>
  );
}

/**
 * Resource names beside the bars, as Project shows them. Drawn after the links with a
 * halo in the background colour so arrows never strike through the text, and flipped left near the end.
 */
function drawResourceLabels(
  ctx: CanvasRenderingContext2D,
  labels: { text: string; left: number; right: number; y: number }[],
  width: number,
  palette: CanvasPalette,
) {
  ctx.font = `11px ${UI_FONT_FAMILY}`;
  ctx.textBaseline = "middle";
  ctx.lineJoin = "round";
  for (const label of labels) {
    const x = resourceLabelX(label.left, label.right, ctx.measureText(label.text).width, width);
    ctx.strokeStyle = palette.labelHalo;
    ctx.lineWidth = 3;
    ctx.strokeText(label.text, x, label.y);
    ctx.fillStyle = palette.label;
    ctx.fillText(label.text, x, label.y);
  }
}

/** Scroll the chart so the row of `uid` is in view after a keyboard move. */
function revealRow(body: HTMLDivElement, rows: ViewRow[], uid: number) {
  const index = rows.findIndex((row) => row.kind === "task" && row.task.uid === uid);
  if (index < 0) return;
  const top = index * ROW;
  if (top < body.scrollTop) body.scrollTop = top;
  else if (top + ROW > body.scrollTop + body.clientHeight) body.scrollTop = top + ROW - body.clientHeight;
}
