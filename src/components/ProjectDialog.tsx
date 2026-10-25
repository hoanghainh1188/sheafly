import { CalendarDays, Check, X } from "lucide-react";
import { useRef, useState, type KeyboardEvent } from "react";

import { appMessages } from "../App.i18n";
import { formatDuration, formatHours, formatInstant, formatMoney } from "../format";
import { getLang, useMessages } from "../i18n";
import type { Command, Project, ProjectStatistics, Snapshot } from "../types";
import { Button } from "./Button";
import { DateTimeField } from "./DateTimeField";
import { FieldsTab } from "./FieldsTab";
import { FocusInput } from "./FocusInput";
import { ForecastTab } from "./ForecastTab";
import { LevelingTab } from "./LevelingTab";
import { Modal } from "./Modal";
import { projectDialogMessages } from "./ProjectDialog.i18n";
import { ProBadge } from "./license/ProBadge";

export type ProjectTab = "general" | "statistics" | "fields" | "leveling" | "forecast";
type Tab = ProjectTab;
const TABS: Tab[] = ["general", "statistics", "fields", "leveling", "forecast"];

/**
 * Changes apply as they are made, so the Gantt previews them; Lưu keeps them and Hủy
 * (✕, Escape) undoes back to the state the dialog opened with. Working time has its own
 * dialog, opened from here once this one is saved.
 */
export function ProjectDialog({
  project,
  statistics,
  changed,
  onSave,
  onCancel,
  send,
  onOpenWorkingTime,
  initialTab = "general",
  error = null,
  capacityPro = true,
}: {
  project: Project;
  /** Computed by the engine; null until loaded. */
  statistics: ProjectStatistics | null;
  changed: boolean;
  onSave: () => void;
  onCancel: () => void;
  send: (command: Command) => Promise<Snapshot | void>;
  onOpenWorkingTime: () => void;
  /** Whether Sheafly Pro unlocks the productivity factor; without it the field only resets. */
  capacityPro?: boolean;
  /** The tab the dialog opens on (Dự án → Trường tùy chỉnh and Thứ tự cân bằng open on theirs). */
  initialTab?: Tab;
  /** The last edit the engine refused, shown in the footer since the banner is behind the dialog. */
  error?: string | null;
}) {
  const m = useMessages(appMessages);
  const p = useMessages(projectDialogMessages);
  const [tab, setTab] = useState<Tab>(initialTab);
  const tabRefs = useRef<Record<Tab, HTMLButtonElement | null>>({ general: null, statistics: null, fields: null, leveling: null, forecast: null });

  function onTabKey(event: KeyboardEvent) {
    const index = TABS.indexOf(tab);
    const next =
      event.key === "ArrowRight" ? TABS[(index + 1) % TABS.length]
      : event.key === "ArrowLeft" ? TABS[(index + TABS.length - 1) % TABS.length]
      : event.key === "Home" ? TABS[0]
      : event.key === "End" ? TABS[TABS.length - 1]
      : null;
    if (!next) return;
    event.preventDefault();
    setTab(next);
    tabRefs.current[next]?.focus();
  }

  return (
    <Modal
      title={m.projectInfo}
      className="wide"
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
          <Button variant="primary" Icon={Check} onClick={onSave}>{m.save}</Button>
        </>
      }
    >
      <div className="view-tabs dialog-tabs" role="tablist" aria-label={p.tabs} onKeyDown={onTabKey}>
        {TABS.map((id) => (
          <button
            key={id}
            ref={(element) => {
              tabRefs.current[id] = element;
            }}
            type="button"
            role="tab"
            id={`project-tab-${id}`}
            aria-selected={tab === id}
            aria-controls="project-panel"
            tabIndex={tab === id ? 0 : -1}
            className={`view-tab${tab === id ? " on" : ""}`}
            onClick={() => setTab(id)}
          >
            {p[id]}
          </button>
        ))}
      </div>
      <div className="dialog-tabpanel" role="tabpanel" id="project-panel" aria-labelledby={`project-tab-${tab}`} tabIndex={0}>
        {tab === "general" ? (
          <GeneralTab project={project} send={send} onOpenWorkingTime={onOpenWorkingTime} capacityPro={capacityPro} />
        ) : tab === "fields" ? (
          <FieldsTab project={project} send={send} />
        ) : tab === "leveling" ? (
          <LevelingTab project={project} send={send} />
        ) : tab === "forecast" ? (
          <ForecastTab project={project} send={send} />
        ) : (
          <StatisticsTab statistics={statistics} />
        )}
      </div>
    </Modal>
  );
}

function GeneralTab({
  project,
  send,
  onOpenWorkingTime,
  capacityPro,
}: {
  project: Project;
  send: (command: Command) => Promise<Snapshot | void>;
  onOpenWorkingTime: () => void;
  capacityPro: boolean;
}) {
  const m = useMessages(appMessages);
  const p = useMessages(projectDialogMessages);
  return (
    <>
      <label htmlFor="project-name">{m.name}</label>
      <input
        id="project-name"
        className="full"
        // Shows the name again after undo or redo inside the dialog.
        key={project.name}
        defaultValue={project.name}
        onBlur={(event) => {
          if (event.target.value !== project.name) void send({ type: "rename_project", name: event.target.value });
        }}
      />
      <label>{m.startDate}</label>
      <DateTimeField
        ariaLabel={m.startDateLabel}
        value={project.start}
        onCommit={(start) => {
          if (start != null) void send({ type: "set_project_start", start });
        }}
      />
      <label>{m.statusDate}</label>
      <DateTimeField
        optional
        ariaLabel={m.statusDate}
        value={project.status_date}
        onCommit={(date) => void send({ type: "set_status_date", date })}
      />
      <label htmlFor="project-calendar">{p.projectCalendar}</label>
      <div className="row">
        <select
          id="project-calendar"
          value={project.calendar_id}
          onChange={(event) => void send({ type: "set_project_calendar", calendar_id: Number(event.target.value) })}
        >
          {project.calendars.map((calendar) => (
            <option key={calendar.id} value={calendar.id}>
              {calendar.name}
            </option>
          ))}
        </select>
        <Button Icon={CalendarDays} title={p.workingTimeTip} onClick={onOpenWorkingTime}>
          {p.workingTime}
        </Button>
      </div>
      <label htmlFor="project-focus" title={p.focusTip}>
        {p.focus}
      </label>
      <div className="row">
        <FocusInput
          id="project-focus"
          value={project.focus_factor ?? 1}
          title={p.focusTip}
          badTitle={p.focusBad}
          locked={!capacityPro}
          lockedTitle={p.focusNeedsPro}
          onCommit={(factor) => (factor == null ? undefined : send({ type: "set_focus_factor", factor }))}
        />
        <ProBadge />
        {!capacityPro && <span className="meta">{p.focusNeedsPro}</span>}
      </div>
      <label>{m.costsGroup}</label>
      <label className="check">
        <input
          type="checkbox"
          checked={project.editable_actual_costs}
          onChange={(event) => void send({ type: "set_editable_actual_costs", editable: event.target.checked })}
        />
        {m.editableActualCosts}
      </label>
    </>
  );
}

/** Current against baseline 0, as Project's statistics; every number comes from the engine. */
function StatisticsTab({ statistics }: { statistics: ProjectStatistics | null }) {
  const p = useMessages(projectDialogMessages);
  if (!statistics) return <p className="meta">{p.noStatistics}</p>;
  const base = statistics.baseline;
  const hours = (minutes: number) => `${formatHours(minutes / 60) || "0"} h`;
  const variance = (current: number, baseline: number | undefined, format: (value: number) => string) =>
    baseline == null ? "" : format(current - baseline);
  const signedDuration = (minutes: number) => (minutes > 0 ? "+" : "") + formatDuration(minutes);
  const signedHours = (minutes: number) => (minutes > 0 ? "+" : "") + hours(minutes);
  const signedMoney = (value: number) => (value > 0 ? "+" : "") + formatMoney(value);
  const rounded = Math.round(statistics.percent_complete);
  const percent = getLang() === "en" ? `${rounded}%` : `${rounded} %`;
  return (
    <>
      <table className="statistics">
        <thead>
          <tr>
            <th scope="col" />
            <th scope="col">{p.columnCurrent}</th>
            <th scope="col">{p.columnBaseline}</th>
            <th scope="col">{p.columnVariance}</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <th scope="row">{p.rowStart}</th>
            <td>{formatInstant(statistics.start)}</td>
            <td>{base ? formatInstant(base.start) : ""}</td>
            <td />
          </tr>
          <tr>
            <th scope="row">{p.rowFinish}</th>
            <td>{formatInstant(statistics.finish)}</td>
            <td>{base ? formatInstant(base.finish) : ""}</td>
            <td />
          </tr>
          <tr>
            <th scope="row">{p.rowDuration}</th>
            <td>{formatDuration(statistics.duration)}</td>
            <td>{base ? formatDuration(base.duration) : ""}</td>
            <td>{variance(statistics.duration, base?.duration, signedDuration)}</td>
          </tr>
          <tr>
            <th scope="row">{p.rowWork}</th>
            <td>{hours(statistics.work)}</td>
            <td>{base ? hours(base.work) : ""}</td>
            <td>{variance(statistics.work, base?.work, signedHours)}</td>
          </tr>
          <tr>
            <th scope="row">{p.rowCost}</th>
            <td>{formatMoney(statistics.cost)}</td>
            <td>{base ? formatMoney(base.cost) : ""}</td>
            <td>{variance(statistics.cost, base?.cost, signedMoney)}</td>
          </tr>
        </tbody>
      </table>
      <table className="statistics">
        <thead>
          <tr>
            <th scope="col" />
            <th scope="col">{p.done}</th>
            <th scope="col">{p.remaining}</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <th scope="row">{p.rowWork}</th>
            <td>{hours(statistics.actual_work)}</td>
            <td>{hours(statistics.remaining_work)}</td>
          </tr>
          <tr>
            <th scope="row">{p.rowCost}</th>
            <td>{formatMoney(statistics.actual_cost)}</td>
            <td>{formatMoney(statistics.remaining_cost)}</td>
          </tr>
        </tbody>
      </table>
      <p>{p.percentComplete(percent)}</p>
      {!base && <p className="meta">{p.noBaseline}</p>}
    </>
  );
}
