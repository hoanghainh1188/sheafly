import { KeyRound, Trash2, X } from "lucide-react";
import { useEffect, useState } from "react";

import { errorMessage, licenseInstall, licenseRemove, licenseStatus } from "../../api";
import { useMessages } from "../../i18n";
import type { LicenseProblem, LicenseView } from "../../types";
import { Button } from "../Button";
import { Modal } from "../Modal";
import { licenseMessages } from "./LicenseDialog.i18n";

type Note = { tone: "ok" | "bad"; text: string } | null;

/**
 * Trợ giúp → License Sheafly Pro: what this computer's license unlocks, entering a key
 * and removing it. The backend checks keys offline and saves only one that works here,
 * so a refused key leaves the saved one as it was.
 */
export function LicenseDialog({ onClose, onChange }: { onClose: () => void; onChange?: (view: LicenseView) => void }) {
  const m = useMessages(licenseMessages);
  const [view, setView] = useState<LicenseView | null>(null);
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<Note>(null);
  // Removing asks once more in the footer: the webview has no native confirm.
  const [confirming, setConfirming] = useState(false);

  useEffect(() => {
    let live = true;
    licenseStatus()
      .then((status) => live && setView(status))
      .catch((error) => live && setNote({ tone: "bad", text: errorMessage(error) }));
    return () => {
      live = false;
    };
  }, []);

  const problemText = (problem: LicenseProblem, day: string | null): string => {
    switch (problem) {
      case "malformed":
        return m.problemMalformed;
      case "newer_format":
        return m.problemNewerFormat;
      case "unknown_key":
        return m.problemUnknownKey;
      case "bad_signature":
        return m.problemBadSignature;
      case "revoked":
        return m.problemRevoked;
      case "expired":
        return m.problemExpired(day ?? "");
      case "updates_ended":
        return m.problemUpdatesEnded(day ?? "");
      case "not_saved":
        return m.problemNotSaved;
    }
  };

  const settle = (next: LicenseView) => {
    setView(next);
    onChange?.(next);
  };

  const activate = async () => {
    setBusy(true);
    setNote(null);
    try {
      const answer = await licenseInstall(key);
      if (answer.ok) {
        settle(answer.view);
        setKey("");
        setNote({ tone: "ok", text: m.activated });
      } else {
        setNote({ tone: "bad", text: problemText(answer.view.problem ?? "malformed", answer.view.problem_day) });
      }
    } catch (error) {
      setNote({ tone: "bad", text: errorMessage(error) });
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    setConfirming(false);
    setBusy(true);
    setNote(null);
    try {
      settle(await licenseRemove());
      setNote({ tone: "ok", text: m.removed });
    } catch (error) {
      setNote({ tone: "bad", text: errorMessage(error) });
    } finally {
      setBusy(false);
    }
  };

  const hasKey = view !== null && view.state !== "none";
  return (
    <Modal
      title={m.title}
      onClose={onClose}
      footer={
        <>
          {/* Mounted empty and filled later, so screen readers announce what changes. */}
          <span className="dialog-foot-note bad" role="alert">
            {confirming ? m.removeConfirm : note?.tone === "bad" ? note.text : ""}
          </span>
          <span className="meta dialog-foot-note" role="status">
            {!confirming && note?.tone === "ok" ? note.text : ""}
          </span>
          <span className="spacer" />
          {hasKey && (
            <Button
              variant="danger"
              Icon={Trash2}
              disabled={busy}
              onClick={() => (confirming ? void remove() : (setNote(null), setConfirming(true)))}
            >
              {confirming ? m.removeYes : m.remove}
            </Button>
          )}
          {confirming ? (
            <Button onClick={() => setConfirming(false)}>{m.keep}</Button>
          ) : (
            <Button Icon={X} onClick={onClose}>
              {m.close}
            </Button>
          )}
        </>
      }
    >
      <div className="license-dialog">
        {view && <LicenseSummary view={view} problemText={problemText} />}
        <label>
          {m.keyLabel}
          <textarea
            value={key}
            rows={4}
            disabled={busy}
            placeholder={m.keyPlaceholder}
            autoCapitalize="off"
            autoComplete="off"
            spellCheck={false}
            onChange={(event) => setKey(event.target.value)}
          />
        </label>
        <div className="license-actions">
          <Button variant="primary" Icon={KeyRound} disabled={busy || key.trim() === ""} onClick={() => void activate()}>
            {view?.state === "active" ? m.replace : m.activate}
          </Button>
        </div>
        <p className="meta">
          {m.offline}
          {view && ` ${m.build(view.build_date)}.`}
        </p>
      </div>
    </Modal>
  );
}

function LicenseSummary({
  view,
  problemText,
}: {
  view: LicenseView;
  problemText: (problem: LicenseProblem, day: string | null) => string;
}) {
  const m = useMessages(licenseMessages);
  if (view.state === "none") {
    return (
      <section className="license-summary" aria-label={m.free}>
        <h3>{m.free}</h3>
        <p className="meta">{m.freeNote}</p>
      </section>
    );
  }
  // A saved key that does not read (damaged, revoked, not ours) has no plan to name.
  const plan = view.plan ? { pro: m.planPro, business: m.planBusiness, trial: m.planTrial }[view.plan] : m.savedKey;
  return (
    <section className={`license-summary ${view.state}`} aria-label={plan}>
      <h3>{plan}</h3>
      {view.name && <p>{m.licensedTo(view.name, view.email ?? "")}</p>}
      {view.seats !== null && view.plan !== "trial" && <p className="meta">{m.seats(view.seats)}</p>}
      {view.expires ? (
        <p className="meta">{m.trialUntil(view.expires)}</p>
      ) : (
        view.updates_until && <p className="meta">{m.updatesUntil(view.updates_until)}</p>
      )}
      {view.state === "inactive" && view.problem && (
        <p className="bad" role="alert">
          {m.notActive}: {problemText(view.problem, view.problem_day)}
        </p>
      )}
    </section>
  );
}
