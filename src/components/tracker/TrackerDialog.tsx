import { ArrowLeft, Check, Download, Plug, Square, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import {
  errorMessage,
  onTrackerProgress,
  trackerCancel,
  trackerCheck,
  trackerForgetKey,
  trackerHasKey,
  trackerPull,
  trackerSaveKey,
  trackerTest,
} from "../../api";
import { useMessages } from "../../i18n";
import type { Command, Project, PullChanges, PullConflict, PullPreview, PullProgress, Resolution, Side, Snapshot } from "../../types";
import { Button } from "../Button";
import { Modal } from "../Modal";
import { trackerDialogMessages } from "./TrackerDialog.i18n";
import { choiceKey, TrackerConflicts } from "./TrackerConflicts";
import { counts, TrackerPreview } from "./TrackerPreview";
import { TrackerSetup } from "./TrackerSetup";
import { complete, formFor, needsAccount, sameSource, sourceOf, type SourceForm } from "./trackerSource";
import { ProBadge } from "../license/ProBadge";

type Stage = { kind: "setup" } | { kind: "fetching"; progress: PullProgress | null } | { kind: "preview"; preview: PullPreview };

/**
 * Tệp → Nhập → Kéo từ task tracker: name the tracker's site, project and filter, fetch the
 * issues, see what pulling them would add, change or mark gone (with problems by issue),
 * and apply it as one `apply_pull` (one undo step). The network and the API key stay in the
 * backend; this dialog only sends the key in once, to the credential store. Not a
 * transaction: nothing in the project changes before Áp dụng, which also saves the source
 * (its own undo step) when the form differs from the one the project holds.
 */
export function TrackerDialog({
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
  const m = useMessages(trackerDialogMessages);
  const [form, setForm] = useState<SourceForm>(() => formFor(project.source));
  const [keyInput, setKeyInput] = useState("");
  const [hasKey, setHasKey] = useState(false);
  const [stage, setStage] = useState<Stage>({ kind: "setup" });
  const [note, setNote] = useState<{ text: string; bad: boolean } | null>(null);
  const [busy, setBusy] = useState(false);
  const sending = useRef(false);
  // What the user chose for each conflict (by `choiceKey`), and what the pull would do under
  // those choices; null while none is made.
  const [choices, setChoices] = useState<Record<string, Side>>({});
  const [resolved, setResolved] = useState<PullChanges | null>(null);
  // The project the preview was made for: a later one means the changes must be asked again.
  const checkedFor = useRef<Project | null>(null);

  const source = sourceOf(form, project.source);
  const ready = complete(source);
  const fetching = stage.kind === "fetching";

  // Whether a key is saved for the site, for the one typed now.
  useEffect(() => {
    if (!source.site) {
      setHasKey(false);
      return;
    }
    let current = true;
    trackerHasKey(source.kind, source.site)
      .then((has) => current && setHasKey(has))
      .catch(() => current && setHasKey(false));
    return () => {
      current = false;
    };
  }, [source.site]);

  // The preview follows the project: an email given to a person, or any edit under the dialog.
  const preview = stage.kind === "preview" ? stage.preview : null;
  useEffect(() => {
    if (!preview) return;
    if (checkedFor.current === project) return;
    checkedFor.current = project;
    let current = true;
    trackerCheck(preview.rows, preview.pulled_at)
      .then((changes) => {
        if (!current) return;
        setStage({ kind: "preview", preview: { ...preview, ...changes } });
        // A choice for a conflict that is gone is dropped.
        const present = new Set(changes.conflicts.map(choiceKey));
        setChoices((now) => Object.fromEntries(Object.entries(now).filter(([key]) => present.has(key))));
      })
      .catch((err) => current && setNote({ text: errorMessage(err), bad: true }));
    return () => {
      current = false;
    };
  }, [project, preview]);

  // What the pull does under the choices made: asked again on every choice and every change.
  const rows = preview?.rows;
  const pulledAt = preview?.pulled_at;
  const resolutions: Resolution[] = Object.entries(choices).map(([key, keep]) => {
    const [issue, field] = key.split("\u0000");
    return { key: issue, field, keep };
  });
  const resolutionsSignature = JSON.stringify(resolutions);
  useEffect(() => {
    if (!rows || pulledAt == null || resolutionsSignature === "[]") {
      setResolved(null);
      return;
    }
    let current = true;
    trackerCheck(rows, pulledAt, JSON.parse(resolutionsSignature) as Resolution[])
      .then((changes) => current && setResolved(changes))
      .catch((err) => current && setNote({ text: errorMessage(err), bad: true }));
    return () => {
      current = false;
    };
  }, [rows, pulledAt, resolutionsSignature, project]);

  function choose(conflict: PullConflict, side: Side) {
    setChoices((now) => ({ ...now, [choiceKey(conflict)]: side }));
  }
  function chooseAll(side: Side) {
    setChoices(Object.fromEntries((preview?.conflicts ?? []).map((conflict) => [choiceKey(conflict), side])));
  }

  async function saveTypedKey() {
    if (!keyInput.trim()) return;
    if (needsAccount(source.kind) && !form.account.trim()) throw new Error(m.needAccount);
    await trackerSaveKey(source.kind, source.site, keyInput, needsAccount(source.kind) ? form.account.trim() : null);
    setKeyInput("");
    setHasKey(true);
  }

  async function test() {
    setBusy(true);
    setNote({ text: m.testing, bad: false });
    try {
      await saveTypedKey();
      setNote({ text: m.testOk(await trackerTest(source)), bad: false });
    } catch (err) {
      setNote({ text: errorMessage(err), bad: true });
    } finally {
      setBusy(false);
    }
  }

  async function fetchIssues() {
    setNote(null);
    setStage({ kind: "fetching", progress: null });
    const stopListening = await onTrackerProgress((progress) =>
      setStage((current) => (current.kind === "fetching" ? { kind: "fetching", progress } : current)),
    );
    try {
      await saveTypedKey();
      const found = await trackerPull(source);
      checkedFor.current = project;
      setStage(found ? { kind: "preview", preview: found } : { kind: "setup" });
    } catch (err) {
      setNote({ text: errorMessage(err), bad: true });
      setStage({ kind: "setup" });
    } finally {
      stopListening();
    }
  }

  async function forgetKey() {
    try {
      await trackerForgetKey(source.kind, source.site);
      setHasKey(false);
      setNote(null);
    } catch (err) {
      setNote({ text: errorMessage(err), bad: true });
    }
  }

  // The preview as it stands under the choices; the list of conflicts stays the full one.
  const shown: PullPreview | null = preview
    ? resolved
      ? { ...preview, changes: resolved.changes, gone: resolved.gone, problems: resolved.problems, bases_changed: resolved.bases_changed }
      : preview
    : null;
  const summary = shown ? counts(shown) : null;
  const changeCount = summary ? summary.added + summary.changed + summary.gone : 0;
  const unresolved = (preview?.conflicts ?? []).some((conflict) => !choices[choiceKey(conflict)]);
  const waitingForCheck = (resolutionsSignature !== "[]") !== (resolved != null);
  const canApply =
    shown != null &&
    shown.problems.length === 0 &&
    !unresolved &&
    !waitingForCheck &&
    (changeCount > 0 || shown.bases_changed) &&
    !busy;

  async function apply() {
    if (!preview || sending.current || !canApply) return;
    sending.current = true;
    setBusy(true);
    let done: Snapshot | void = undefined;
    if (!sameSource(project.source, source)) done = await send({ type: "set_source", source });
    else done = {} as Snapshot;
    if (done) done = await send({ type: "apply_pull", rows: preview.rows, pulled_at: preview.pulled_at, resolutions });
    sending.current = false;
    setBusy(false);
    if (done) onClose();
  }

  const close = () => {
    if (sending.current) return;
    if (fetching) void trackerCancel();
    onClose();
  };

  const status = error ?? (note?.bad ? note.text : null);
  const foot =
    status != null ? (
      <span className="dialog-foot-note bad" role="alert">
        {status}
      </span>
    ) : (
      <span className="meta dialog-foot-note" role="status">
        {stage.kind === "fetching" ? m.progress(stage.progress?.fetched ?? 0, stage.progress?.total ?? null) : (note?.text ?? "")}
      </span>
    );

  const footer = (
    <>
      {foot}
      {stage.kind === "setup" && (
        <>
          <Button Icon={X} disabled={busy} onClick={close}>
            {m.cancel}
          </Button>
          <Button Icon={Plug} disabled={busy || !ready} onClick={() => void test()}>
            {m.test}
          </Button>
          <Button variant="primary" Icon={Download} disabled={busy || !ready} onClick={() => void fetchIssues()}>
            {m.fetch}
          </Button>
        </>
      )}
      {stage.kind === "fetching" && (
        <Button Icon={Square} onClick={() => void trackerCancel()}>
          {m.stopFetch}
        </Button>
      )}
      {stage.kind === "preview" && (
        <>
          <Button Icon={X} disabled={busy} onClick={close}>
            {m.cancel}
          </Button>
          <Button Icon={ArrowLeft} disabled={busy} onClick={() => setStage({ kind: "setup" })}>
            {m.back}
          </Button>
          <Button variant="primary" Icon={Check} disabled={!canApply} onClick={() => void apply()}>
            {m.apply(changeCount)}
          </Button>
        </>
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
      {stage.kind !== "preview" ? (
        <TrackerSetup
          form={form}
          onForm={setForm}
          keyInput={keyInput}
          onKeyInput={setKeyInput}
          account={form.account}
          onAccount={(account) => setForm({ ...form, account })}
          hasKey={hasKey}
          onForgetKey={() => void forgetKey()}
          disabled={fetching || busy}
        />
      ) : (
        <>
          {stage.preview.conflicts.length > 0 && (
            <TrackerConflicts
              project={project}
              conflicts={stage.preview.conflicts}
              choices={choices}
              onChoose={choose}
              onChooseAll={chooseAll}
            />
          )}
          <TrackerPreview
            project={project}
            preview={shown ?? stage.preview}
            onMatchEmail={(uid, email) => void send({ type: "set_resource_email", uid, email })}
          />
        </>
      )}
    </Modal>
  );
}
