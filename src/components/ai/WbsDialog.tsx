import { Check, Sparkles, Square, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { aiCancel, aiWbs, checkImport, errorMessage } from "../../api";
import { formatDuration } from "../../format";
import { useMessages } from "../../i18n";
import type { Command, ImportProblem, ImportTask, Project, Snapshot, Task, WbsReport } from "../../types";
import { Button } from "../Button";
import { Modal } from "../Modal";
import { ProBadge } from "../license/ProBadge";
import { wbsDialogMessages } from "./WbsDialog.i18n";

/** What `wbs_request` takes at most (the Pro crate refuses more). */
export const MAX_DESCRIPTION = 8_000;

type Phase =
  | { kind: "idle" }
  | { kind: "running" }
  | { kind: "stopped" }
  | { kind: "failed"; message: string }
  | { kind: "done"; report: WbsReport };

interface WbsDialogProps {
  project: Project;
  /** Where the tasks go: after this task's subtree, or at the end when null. */
  afterTask: Task | null;
  /** The last command the engine refused. */
  error: string | null;
  send: (command: Command) => Promise<Snapshot | void>;
  onClose: () => void;
}

/** A row is a group when the next row is deeper. */
function isGroup(tasks: ImportTask[], at: number): boolean {
  return (tasks[at + 1]?.level ?? 0) > (tasks[at]?.level ?? 0);
}

/**
 * Kind 4 of AI recommendations (phase 22 PR E): the user describes some work, the AI answers
 * with the rows of one `import_rows`, and the engine checks them (`check_import`, problems by
 * row, again on every project change) as for a table import. Nothing changes before Import,
 * one undo step. Closing the dialog stops a request still running.
 */
export function WbsDialog({ project, afterTask, error, send, onClose }: WbsDialogProps) {
  const m = useMessages(wbsDialogMessages);
  const [description, setDescription] = useState("");
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });
  const [check, setCheck] = useState<{ problems: ImportProblem[]; ready: boolean }>({ problems: [], ready: false });
  const [busy, setBusy] = useState(false);
  const request = useRef(0);
  const running = phase.kind === "running";
  const runningRef = useRef(false);
  runningRef.current = running;
  const afterUid = afterTask?.uid ?? null;
  const tasks = phase.kind === "done" ? phase.report.tasks : null;

  useEffect(
    () => () => {
      request.current += 1;
      if (runningRef.current) void aiCancel().catch(() => undefined);
    },
    [],
  );

  // The engine checks the rows against the project as it is, whenever either changes.
  useEffect(() => {
    if (!tasks) return;
    let current = true;
    setCheck({ problems: [], ready: false });
    checkImport(afterUid, [], tasks)
      .then((problems) => current && setCheck({ problems, ready: true }))
      .catch((err) => current && setCheck({ problems: [{ table: "tasks", row: null, column: "row", message: errorMessage(err) }], ready: true }));
    return () => {
      current = false;
    };
  }, [tasks, project, afterUid]);

  async function build() {
    const id = ++request.current;
    setPhase({ kind: "running" });
    try {
      const report = await aiWbs(description);
      if (id === request.current) setPhase(report ? { kind: "done", report } : { kind: "stopped" });
    } catch (err) {
      if (id === request.current) setPhase({ kind: "failed", message: errorMessage(err) });
    }
  }

  function stop() {
    request.current += 1;
    setPhase({ kind: "stopped" });
    void aiCancel().catch(() => undefined);
  }

  const canImport = tasks != null && tasks.length > 0 && check.ready && check.problems.length === 0 && !busy;

  async function submit() {
    if (!tasks || !canImport) return;
    setBusy(true);
    const done = await send({ type: "import_rows", after_uid: afterUid, resources: [], tasks });
    setBusy(false);
    if (done) onClose();
  }

  const bad = new Map(check.problems.filter((problem) => problem.row != null).map((problem) => [problem.row as number, problem.message]));
  const status = error ?? (phase.kind === "failed" ? m.failed(phase.message) : null);
  const footer = (
    <>
      {status ? (
        <span className="dialog-foot-note bad" role="alert">
          {status}
        </span>
      ) : (
        <span className="meta dialog-foot-note" role="status">
          {running ? m.running : phase.kind === "stopped" ? m.stopped : tasks && !check.ready ? m.checking : canImport ? m.ready(tasks.length) : ""}
        </span>
      )}
      <Button Icon={X} disabled={busy} onClick={onClose}>
        {m.cancel}
      </Button>
      <Button variant="primary" Icon={Check} disabled={!canImport} onClick={() => void submit()}>
        {m.importN(tasks?.length ?? 0)}
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
      className="wide wbs-dialog"
      onClose={onClose}
      footer={footer}
    >
      <label className="wbs-description">
        {m.description}
        <textarea
          value={description}
          maxLength={MAX_DESCRIPTION}
          rows={6}
          disabled={running}
          onChange={(event) => setDescription(event.target.value)}
        />
      </label>
      <p className="meta">{m.descriptionHint(MAX_DESCRIPTION)}</p>
      <div className="ai-panel-actions">
        {running ? (
          <Button Icon={Square} onClick={stop}>
            {m.stop}
          </Button>
        ) : (
          <Button Icon={Sparkles} disabled={description.trim() === ""} onClick={() => void build()}>
            {tasks ? m.buildAgain : m.build}
          </Button>
        )}
      </div>
      {phase.kind === "done" && tasks && (
        <>
          {phase.report.note && <p className="evm-note">{m.note(phase.report.note)}</p>}
          <p className="meta">{afterTask ? m.afterTask(afterTask.name) : m.atEnd}</p>
          <div className="suggestion-wrap import-preview">
            <table className="suggestions" aria-label={m.preview}>
              <thead>
                <tr>
                  <th scope="col">{m.row}</th>
                  <th scope="col">{m.name}</th>
                  <th scope="col">{m.effort}</th>
                  <th scope="col">{m.duration}</th>
                  <th scope="col">{m.people}</th>
                  <th scope="col">{m.after}</th>
                </tr>
              </thead>
              <tbody>
                {tasks.map((task, at) => (
                  <tr key={at} className={bad.has(at) ? "bad" : undefined} title={bad.get(at)}>
                    <th scope="row">{at + 1}</th>
                    <td style={{ paddingInlineStart: `${0.5 + (task.level ?? 0) * 1.25}rem` }}>
                      {isGroup(tasks, at) ? <strong>{task.name}</strong> : task.name}
                    </td>
                    <td>{task.work != null ? formatDuration(task.work) : ""}</td>
                    <td>{task.duration != null ? formatDuration(task.duration) : isGroup(tasks, at) ? m.group : ""}</td>
                    <td>{(task.assignees ?? []).map((person) => person.name).join(", ")}</td>
                    <td>{(task.after ?? []).map((row) => row + 1).join(", ")}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          {check.problems.length > 0 && (
            <section>
              <h3 className="dialog-subhead">{m.problems}</h3>
              <ul aria-label={m.problems} className="import-problems">
                {check.problems.map((problem, at) => (
                  <li key={at}>{m.problemAt(problem.row == null ? null : problem.row + 1, problem.message)}</li>
                ))}
              </ul>
            </section>
          )}
        </>
      )}
    </Modal>
  );
}
