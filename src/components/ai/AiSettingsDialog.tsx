import { KeyRound, PlugZap, Trash2, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { aiForgetKey, aiHasKey, aiSaveKey, aiSetSettings, aiSettings, aiTest, errorMessage } from "../../api";
import { useMessages } from "../../i18n";
import type { AiSettings } from "../../types";
import { Button } from "../Button";
import { ProBadge } from "../license/ProBadge";
import { Modal } from "../Modal";
import { aiSettingsMessages } from "./AiSettingsDialog.i18n";

/** The models offered by name (D8); anything else is typed as an ID. */
export const AI_MODELS = ["claude-opus-5-5", "claude-sonnet-5-5"] as const;
const OTHER = "other";

type Note = { tone: "ok" | "bad"; text: string } | null;

interface AiSettingsDialogProps {
  onClose: () => void;
  /** Whether this computer's license includes AI recommendations. */
  aiPro: boolean;
  /** Open Help → Sheafly Pro License, for the connection check without a license. */
  onOpenLicense: () => void;
}

/**
 * Trợ giúp → Gợi ý AI (phase 22, PR B): turning AI on (only after the data notice), the
 * Anthropic API key, the model and masking. Each change is saved at once; the key goes to the
 * system's credential store and the dialog only learns whether one is saved.
 */
export function AiSettingsDialog({ onClose, aiPro, onOpenLicense }: AiSettingsDialogProps) {
  const m = useMessages(aiSettingsMessages);
  const [settings, setSettings] = useState<AiSettings | null>(null);
  const [hasKey, setHasKey] = useState(false);
  const [keyInput, setKeyInput] = useState("");
  const [otherModel, setOtherModel] = useState<string | null>(null);
  const [notice, setNotice] = useState<"asking" | "showing" | null>(null);
  const [busy, setBusy] = useState(false);
  const [testing, setTesting] = useState(false);
  const [note, setNote] = useState<Note>(null);
  // Settings changes go out one at a time, each built on the one before, so a quick second
  // change (a click right after typing a model ID) is never lost or sent on stale settings.
  const latest = useRef<AiSettings | null>(null);
  const queue = useRef<Promise<void>>(Promise.resolve());

  useEffect(() => {
    let live = true;
    Promise.all([aiSettings(), aiHasKey()])
      .then(([loaded, saved]) => {
        if (!live) return;
        latest.current = loaded;
        setSettings(loaded);
        setHasKey(saved);
        if (!(AI_MODELS as readonly string[]).includes(loaded.model)) setOtherModel(loaded.model);
      })
      .catch((error) => live && setNote({ tone: "bad", text: errorMessage(error) }));
    return () => {
      live = false;
    };
  }, []);

  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setNote(null);
    try {
      await action();
    } catch (error) {
      setNote({ tone: "bad", text: errorMessage(error) });
    } finally {
      setBusy(false);
    }
  };

  /** Show the change at once and send it after those before; a refusal puts back what the
   * backend holds, which is what is in use. */
  const save = (change: Partial<AiSettings>) => {
    if (!latest.current) return;
    const next = { ...latest.current, ...change };
    latest.current = next;
    setSettings(next);
    setNote(null);
    queue.current = queue.current.then(async () => {
      try {
        latest.current = await aiSetSettings(next);
      } catch (error) {
        setNote({ tone: "bad", text: errorMessage(error) });
        latest.current = await aiSettings().catch(() => latest.current);
      }
      setSettings(latest.current);
    });
  };

  const toggle = (enabled: boolean) => {
    if (enabled && !settings?.notice_accepted) {
      setNotice("asking");
      return;
    }
    save({ enabled });
  };

  const accept = () => {
    setNotice(null);
    save({ enabled: true, notice_accepted: true });
  };

  const chooseModel = (choice: string) => {
    if (choice === OTHER) {
      setOtherModel(otherModel ?? "");
      return;
    }
    setOtherModel(null);
    save({ model: choice });
  };

  const commitOtherModel = () => {
    const typed = otherModel?.trim() ?? "";
    if (typed !== "" && typed !== latest.current?.model) save({ model: typed });
  };

  const saveKey = () =>
    run(async () => {
      await aiSaveKey(keyInput);
      setKeyInput("");
      setHasKey(true);
      setNote({ tone: "ok", text: m.keyStored });
    });

  const forgetKey = () =>
    run(async () => {
      await aiForgetKey();
      setHasKey(false);
      setNote({ tone: "ok", text: m.keyForgotten });
    });

  const test = () => {
    if (!aiPro) {
      onOpenLicense();
      return;
    }
    setTesting(true);
    void run(async () => {
      const found = await aiTest();
      setNote({ tone: "ok", text: m.testOk(found.model, found.input_tokens + found.output_tokens) });
    }).finally(() => setTesting(false));
  };

  const modelChoice = otherModel !== null ? OTHER : (settings?.model ?? AI_MODELS[0]);
  return (
    <Modal
      title={m.title}
      onClose={onClose}
      footer={
        <>
          <span className="dialog-foot-note bad" role="alert">
            {note?.tone === "bad" ? note.text : ""}
          </span>
          <span className="meta dialog-foot-note" role="status">
            {testing ? m.testing : note?.tone === "ok" ? note.text : ""}
          </span>
          <span className="spacer" />
          <Button Icon={PlugZap} disabled={busy || !settings?.enabled || !hasKey} onClick={test}>
            {m.test}
            <ProBadge />
          </Button>
          <Button Icon={X} onClick={onClose}>
            {m.close}
          </Button>
        </>
      }
    >
      <div className="ai-settings">
        <p>{m.intro}</p>
        <p className="meta">{m.comingSoon}</p>
        <label className="ai-check">
          <input
            type="checkbox"
            checked={settings?.enabled ?? false}
            disabled={settings === null || notice === "asking"}
            onChange={(event) => toggle(event.target.checked)}
          />
          {m.enable}
          <ProBadge />
        </label>
        {notice !== null ? (
          <section className="ai-notice" aria-label={m.noticeTitle}>
            <h3>{m.noticeTitle}</h3>
            <p>{m.noticeSent}</p>
            <p>{m.noticeNotSent}</p>
            <div className="ai-actions">
              {notice === "asking" ? (
                <>
                  <Button onClick={() => setNotice(null)}>{m.decline}</Button>
                  <Button variant="primary" onClick={accept}>
                    {m.accept}
                  </Button>
                </>
              ) : (
                <Button onClick={() => setNotice(null)}>{m.hideNotice}</Button>
              )}
            </div>
          </section>
        ) : (
          <button type="button" className="link-button" onClick={() => setNotice("showing")}>
            {m.showNotice}
          </button>
        )}
        <label>
          {m.keyLabel}
          <input
            type="password"
            value={keyInput}
            disabled={busy}
            placeholder={m.keyPlaceholder}
            autoCapitalize="off"
            autoComplete="off"
            spellCheck={false}
            onChange={(event) => setKeyInput(event.target.value)}
          />
        </label>
        <div className="ai-actions">
          <span className="meta">{hasKey ? m.keySaved : m.noKey}</span>
          {hasKey && (
            <Button variant="danger" Icon={Trash2} disabled={busy} onClick={() => void forgetKey()}>
              {m.forgetKey}
            </Button>
          )}
          <Button variant="primary" Icon={KeyRound} disabled={busy || keyInput.trim() === ""} onClick={() => void saveKey()}>
            {m.saveKey}
          </Button>
        </div>
        <label>
          {m.model}
          <select value={modelChoice} disabled={settings === null} onChange={(event) => chooseModel(event.target.value)}>
            <option value={AI_MODELS[0]}>{m.modelOpus}</option>
            <option value={AI_MODELS[1]}>{m.modelSonnet}</option>
            <option value={OTHER}>{m.modelOther}</option>
          </select>
        </label>
        {otherModel !== null && (
          <label>
            {m.modelId}
            <input
              value={otherModel}
              autoCapitalize="off"
              spellCheck={false}
              onChange={(event) => setOtherModel(event.target.value)}
              onBlur={commitOtherModel}
              onKeyDown={(event) => event.key === "Enter" && commitOtherModel()}
            />
          </label>
        )}
        <label className="ai-check">
          <input
            type="checkbox"
            checked={settings?.mask ?? false}
            disabled={settings === null}
            onChange={(event) => save({ mask: event.target.checked })}
          />
          {m.mask}
        </label>
      </div>
    </Modal>
  );
}
