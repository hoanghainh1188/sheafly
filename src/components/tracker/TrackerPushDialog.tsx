import { ArrowLeft, Check, Square, Upload, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import {
  errorMessage,
  onTrackerProgress,
  onTrackerPushProgress,
  trackerCancel,
  trackerHasKey,
  trackerPush,
  trackerPushPreview,
} from "../../api";
import { useMessages } from "../../i18n";
import type { Command, Project, PushOptions, PushPreview, PushProgress, PushReport, Snapshot } from "../../types";
import { Button } from "../Button";
import { Modal } from "../Modal";
import { TrackerPushPreview } from "./TrackerPushPreview";
import { trackerPushMessages } from "./TrackerPushDialog.i18n";
import { ProBadge } from "../license/ProBadge";

type Stage =
  | { kind: "options" }
  | { kind: "previewing"; fetched: number; total: number | null }
  | { kind: "preview"; preview: PushPreview; options: PushOptions }
  | { kind: "pushing"; progress: PushProgress }
  | { kind: "result"; report: PushReport };

const DEFAULT_OPTIONS: PushOptions = { start: true, due: true, assignee: false, overwrite: false };

/**
 * Tệp → Xuất → Đẩy ngày về task tracker: write the dates the engine computed (and,
 * if asked, the assignee) to the issues pulled from the project's source. It always shows
 * a preview of what will change at the source first (fetched fresh, so it is what the
 * source holds now), writes only after the user presses the button that says so, and
 * remembers the days that went through (`record_push`) so the next pull does not read them
 * back. Fields at the source are filled when empty and replaced only when asked; nothing
 * is ever cleared.
 */
export function TrackerPushDialog({
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
  const m = useMessages(trackerPushMessages);
  const source = project.source;
  const [options, setOptions] = useState<PushOptions>(DEFAULT_OPTIONS);
  const [stage, setStage] = useState<Stage>({ kind: "options" });
  const [hasKey, setHasKey] = useState<boolean | null>(null);
  const [note, setNote] = useState<string | null>(null);
  // The project the preview was made for: a different one makes the preview stale.
  const previewedFor = useRef<Project | null>(null);

  useEffect(() => {
    if (!source) return;
    let current = true;
    trackerHasKey(source.kind, source.site)
      .then((has) => current && setHasKey(has))
      .catch(() => current && setHasKey(false));
    return () => {
      current = false;
    };
  }, [source]);

  // Any change to the project under a preview makes it stale: ask again.
  useEffect(() => {
    if (stage.kind === "preview" && previewedFor.current !== project) {
      setStage({ kind: "options" });
      setNote(m.changed);
    }
  }, [project, stage.kind, m.changed]);

  const chosen = options.start || options.due || options.assignee;

  async function previewChanges() {
    if (!source) return;
    setNote(null);
    setStage({ kind: "previewing", fetched: 0, total: null });
    const stopListening = await onTrackerProgress((progress) =>
      setStage((current) => (current.kind === "previewing" ? { kind: "previewing", fetched: progress.fetched, total: progress.total } : current)),
    );
    try {
      const preview = await trackerPushPreview(source, options);
      previewedFor.current = project;
      setStage(preview ? { kind: "preview", preview, options } : { kind: "options" });
    } catch (err) {
      setNote(errorMessage(err));
      setStage({ kind: "options" });
    } finally {
      stopListening();
    }
  }

  async function write(preview: PushPreview, used: PushOptions) {
    if (!source) return;
    setNote(null);
    setStage({ kind: "pushing", progress: { done: 0, total: preview.plan.changes.length } });
    const stopListening = await onTrackerPushProgress((progress) =>
      setStage((current) => (current.kind === "pushing" ? { kind: "pushing", progress } : current)),
    );
    try {
      const report = await trackerPush(source, used);
      // What went through is remembered right away, so the next pull knows those days.
      if (report.entries.length > 0) await send({ type: "record_push", entries: report.entries });
      setStage({ kind: "result", report });
    } catch (err) {
      setNote(errorMessage(err));
      setStage({ kind: "options" });
    } finally {
      stopListening();
    }
  }

  const busy = stage.kind === "previewing" || stage.kind === "pushing";
  // While writing, closing would lose track of what went through: only Stop ends it.
  const close = () => {
    if (stage.kind === "pushing") return;
    if (stage.kind === "previewing") void trackerCancel();
    onClose();
  };

  const status = error ?? note;
  const progressText =
    stage.kind === "previewing"
      ? m.progress(stage.fetched, stage.total)
      : stage.kind === "pushing"
        ? m.pushing(stage.progress.done, stage.progress.total)
        : "";
  const foot = status ? (
    <span className="dialog-foot-note bad" role="alert">
      {status}
    </span>
  ) : (
    <span className="meta dialog-foot-note" role="status">
      {progressText}
    </span>
  );

  const footer = (
    <>
      {foot}
      {stage.kind === "options" && (
        <>
          <Button Icon={X} onClick={close}>
            {m.cancel}
          </Button>
          <Button variant="primary" Icon={Upload} disabled={!source || !chosen || hasKey !== true} onClick={() => void previewChanges()}>
            {m.preview}
          </Button>
        </>
      )}
      {busy && (
        <Button Icon={Square} onClick={() => void trackerCancel()}>
          {m.stop}
        </Button>
      )}
      {stage.kind === "preview" && (
        <>
          <Button Icon={X} onClick={close}>
            {m.cancel}
          </Button>
          <Button Icon={ArrowLeft} onClick={() => setStage({ kind: "options" })}>
            {m.back}
          </Button>
          <Button
            variant="primary"
            Icon={Check}
            disabled={stage.preview.plan.changes.length === 0}
            onClick={() => void write(stage.preview, stage.options)}
          >
            {m.write(stage.preview.plan.changes.length)}
          </Button>
        </>
      )}
      {stage.kind === "result" && (
        <Button variant="primary" Icon={Check} onClick={onClose}>
          {m.close}
        </Button>
      )}
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
      onClose={close}
      footer={footer}
    >
      {!source ? (
        <p>{m.noSource}</p>
      ) : stage.kind === "preview" ? (
        <TrackerPushPreview preview={stage.preview} />
      ) : stage.kind === "result" ? (
        <PushResult report={stage.report} />
      ) : (
        <div className="tracker-setup">
          <p className="meta">{m.to(source.site, source.project_key)}</p>
          {hasKey === false && <p className="bad">{m.noKey(source.site)}</p>}
          <fieldset className="tracker-statuses">
            <legend>{m.what}</legend>
            {(["start", "due", "assignee"] as const).map((field) => (
              <label key={field}>
                <input
                  type="checkbox"
                  checked={options[field]}
                  disabled={busy}
                  onChange={(event) => setOptions({ ...options, [field]: event.target.checked })}
                />{" "}
                {m[field]}
              </label>
            ))}
            {!chosen && <p className="meta">{m.nothingChosen}</p>}
            {options.due && <p className="meta">{m.dueWarning}</p>}
          </fieldset>
          <fieldset className="tracker-statuses">
            <legend>{m.mode}</legend>
            {([false, true] as const).map((overwrite) => (
              <label key={String(overwrite)}>
                <input
                  type="radio"
                  name="push-mode"
                  checked={options.overwrite === overwrite}
                  disabled={busy}
                  onChange={() => setOptions({ ...options, overwrite })}
                />{" "}
                {overwrite ? m.overwrite : m.fillEmpty}
              </label>
            ))}
          </fieldset>
        </div>
      )}
    </Modal>
  );
}

/** What a push did: how many went through, which failed and why, and what was not sent. */
function PushResult({ report }: { report: PushReport }) {
  const m = useMessages(trackerPushMessages);
  return (
    <div className="tracker-preview">
      <p role="status">{m.resultDone(report.done.length)}</p>
      {report.stopped && <p className="bad">{m.resultStopped(report.stopped)}</p>}
      {report.failed.length > 0 && (
        <section>
          <h3 className="dialog-subhead">{m.resultFailed(report.failed.length)}</h3>
          <ul aria-label={m.resultFailed(report.failed.length)} className="import-problems">
            {report.failed.map((failure) => (
              <li key={failure.key}>
                {failure.key}: {failure.message}
              </li>
            ))}
          </ul>
        </section>
      )}
      {report.not_sent.length > 0 && <p className="meta">{m.resultNotSent(report.not_sent.length)}</p>}
    </div>
  );
}
