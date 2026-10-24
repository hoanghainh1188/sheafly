import { Check, Plug, RotateCcw, X } from "lucide-react";
import { useState } from "react";

import { errorMessage, trackerTest } from "../../api";
import { useMessages } from "../../i18n";
import type { Command, OnConflict, Project, Snapshot, Source, SyncDirection, TrackerKind } from "../../types";
import { Button } from "../Button";
import { Modal } from "../Modal";
import { kindOf, toRows, toSettings, type Row } from "./syncSettings";
import { trackerSettingsMessages } from "./TrackerSettingsDialog.i18n";
import { ProBadge } from "../license/ProBadge";

/** What each tracker reads a field from when nothing else is named. */
const USUAL: Record<TrackerKind, Record<string, string>> = {
  backlog: { work: "estimatedHours", start: "startDate", due: "dueDate" },
  // Jira has no standard start date: a start comes only from a date field the settings name.
  jira: { work: "originalEstimate", due: "duedate" },
};

const DIRECTIONS: SyncDirection[] = ["both", "pull", "push", "off"];
const CONFLICTS: OnConflict[] = ["ask", "source", "sheafly"];

function numberOf(text: string): number | null {
  const value = Number(text.trim().replace(",", "."));
  return text.trim() !== "" && Number.isFinite(value) ? value : null;
}

/**
 * Dự án → Đồng bộ task tracker: how each field syncs with the project's source. Per field,
 * a direction (both, pull only, push only, off), what to do when both sides changed it, and
 * a field at the source to read or write when it is not the usual one; plus the constants
 * (percent of an in-progress issue, the hours of a date, hours per point). Saved with
 * `set_source`; the engine checks it. "Kiểm tra với nguồn" asks the tracker whether the
 * named fields exist and are the right kind, without saving.
 */
export function TrackerSettingsDialog({
  project,
  error,
  send,
  onClose,
}: {
  project: Project;
  /** The last command the engine refused. */
  error: string | null;
  send: (command: Command) => Promise<Snapshot | void>;
  onClose: () => void;
}) {
  const m = useMessages(trackerSettingsMessages);
  const source = project.source;
  const [rows, setRows] = useState<Row[]>(() => (source ? toRows(source.sync, project) : []));
  const [percent, setPercent] = useState(String(source?.sync.in_progress_percent ?? 50));
  const [startHour, setStartHour] = useState(String(source?.sync.start_hour ?? 8));
  const [dueHour, setDueHour] = useState(String(source?.sync.due_hour ?? 17));
  const [perPoint, setPerPoint] = useState(String(source?.hours_per_point ?? 4).replace(".", ","));
  const [note, setNote] = useState<{ text: string; bad: boolean } | null>(null);
  const [busy, setBusy] = useState(false);

  const percentNumber = numberOf(percent);
  const startNumber = numberOf(startHour);
  const dueNumber = numberOf(dueHour);
  const perPointNumber = numberOf(perPoint);
  const valid =
    percentNumber != null && Number.isInteger(percentNumber) && percentNumber >= 1 && percentNumber <= 99 &&
    startNumber != null && Number.isInteger(startNumber) && startNumber >= 0 && startNumber <= 23 &&
    dueNumber != null && Number.isInteger(dueNumber) && dueNumber >= 0 && dueNumber <= 23 &&
    perPointNumber != null && perPointNumber > 0;

  function edited(): Source | null {
    if (!source || !valid) return null;
    return {
      ...source,
      hours_per_point: perPointNumber,
      sync: toSettings(rows, { inProgressPercent: percentNumber, startHour: startNumber, dueHour: dueNumber }),
    };
  }

  const change = (field: string, patch: Partial<Row>) =>
    setRows((now) => now.map((row) => (row.field === field ? { ...row, ...patch } : row)));

  async function test() {
    const next = edited();
    if (!next) return;
    setBusy(true);
    setNote({ text: m.testing, bad: false });
    try {
      await trackerTest(next);
      setNote({ text: m.testOk, bad: false });
    } catch (err) {
      setNote({ text: errorMessage(err), bad: true });
    } finally {
      setBusy(false);
    }
  }

  async function save() {
    const next = edited();
    if (!next) return;
    setBusy(true);
    const done = await send({ type: "set_source", source: next });
    setBusy(false);
    if (done) onClose();
  }

  function reset() {
    if (!source) return;
    setRows(toRows({ ...source.sync, fields: [] }, project));
    setPercent("50");
    setStartHour("8");
    setDueHour("17");
    setNote(null);
  }

  const label = (field: string) => {
    if (field.startsWith("field:")) {
      const id = Number(field.slice("field:".length));
      return m.custom(project.fields.find((item) => item.id === id)?.name ?? field);
    }
    return (m as Record<string, unknown>)[`label_${field}`] as string;
  };
  const directionLabel = (direction: SyncDirection) => m[direction];
  const conflictLabel = (conflict: OnConflict) => m[conflict];

  const status = error ?? (note?.bad ? note.text : null);
  const footer = (
    <>
      {status ? (
        <span className="dialog-foot-note bad" role="alert">
          {status}
        </span>
      ) : (
        <span className="meta dialog-foot-note" role="status">
          {valid ? (note?.text ?? "") : m.badNumber}
        </span>
      )}
      <Button Icon={RotateCcw} disabled={busy || !source} onClick={reset}>
        {m.reset}
      </Button>
      <Button Icon={Plug} disabled={busy || !valid || !source} onClick={() => void test()}>
        {m.test}
      </Button>
      <Button Icon={X} disabled={busy} onClick={onClose}>
        {m.cancel}
      </Button>
      <Button variant="primary" Icon={Check} disabled={busy || !valid || !source} onClick={() => void save()}>
        {m.save}
      </Button>
    </>
  );

  return (
    <Modal
      title={
        <>
          {m.title}
          <ProBadge />
        </>
      }
      className="wide tracker-dialog"
      onClose={onClose}
      footer={footer}
    >
      {!source ? (
        <p>{m.noSource}</p>
      ) : (
        <div className="tracker-setup">
          <p className="meta">{m.intro}</p>
          <p className="meta">{source.kind === "jira" ? m.refHintJira : m.refHint}</p>
          <div className="suggestion-wrap">
            <table className="suggestions" aria-label={m.title}>
              <thead>
                <tr>
                  <th scope="col">{m.field}</th>
                  <th scope="col">{m.direction}</th>
                  <th scope="col">{m.onConflict}</th>
                  <th scope="col">{m.readFrom}</th>
                  <th scope="col">{m.writeTo}</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((row) => {
                  const kind = kindOf(row.field);
                  const name = label(row.field);
                  return (
                    <tr key={row.field}>
                      <th scope="row">{name}</th>
                      <td>
                        <select
                          aria-label={`${name}: ${m.direction}`}
                          value={row.direction}
                          onChange={(event) => change(row.field, { direction: event.target.value as SyncDirection })}
                        >
                          {DIRECTIONS.filter((direction) => kind.directions.includes(direction)).map((direction) => (
                            <option key={direction} value={direction}>
                              {directionLabel(direction)}
                            </option>
                          ))}
                        </select>
                      </td>
                      <td>
                        {kind.conflicts && (
                          <select
                            aria-label={`${name}: ${m.onConflict}`}
                            value={row.onConflict}
                            onChange={(event) => change(row.field, { onConflict: event.target.value as OnConflict })}
                          >
                            {CONFLICTS.map((conflict) => (
                              <option key={conflict} value={conflict}>
                                {conflictLabel(conflict)}
                              </option>
                            ))}
                          </select>
                        )}
                      </td>
                      <td>
                        {kind.readsFrom && (
                          <input
                            aria-label={`${name}: ${m.readFrom}`}
                            value={row.sourceField}
                            placeholder={m.readPlaceholder(USUAL[source.kind][row.field] ?? "custom:…")}
                            spellCheck={false}
                            onChange={(event) => change(row.field, { sourceField: event.target.value })}
                          />
                        )}
                      </td>
                      <td>
                        {kind.writesTo && (
                          <input
                            aria-label={`${name}: ${m.writeTo}`}
                            value={row.pushField}
                            placeholder={m.writePlaceholder}
                            title={m.plannedHint}
                            spellCheck={false}
                            onChange={(event) => change(row.field, { pushField: event.target.value })}
                          />
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
          <p className="meta">{m.plannedHint}</p>
          <fieldset className="tracker-statuses">
            <legend>{m.constants}</legend>
            <label>
              {m.inProgress} <input aria-label={m.inProgress} value={percent} onChange={(event) => setPercent(event.target.value)} />
            </label>
            <label>
              {m.startHour} <input aria-label={m.startHour} value={startHour} onChange={(event) => setStartHour(event.target.value)} />
            </label>
            <label>
              {m.dueHour} <input aria-label={m.dueHour} value={dueHour} onChange={(event) => setDueHour(event.target.value)} />
            </label>
            <label>
              {m.hoursPerPoint} <input aria-label={m.hoursPerPoint} value={perPoint} onChange={(event) => setPerPoint(event.target.value)} />
            </label>
          </fieldset>
        </div>
      )}
    </Modal>
  );
}

