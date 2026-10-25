import { KeyRound, Settings2, Sparkles, Square, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import { aiCancel, aiRisks, aiSettings, errorMessage } from "../../api";
import { getLang, useMessages } from "../../i18n";
import type { AiSettings, Project, RiskCard, RiskReport, RiskSeverity } from "../../types";
import { Button } from "../Button";
import { ProBadge } from "../license/ProBadge";
import { adviceNames, adviceParts, refUid, type AdviceNames } from "./adviceText";
import { recommendationsMessages } from "./RecommendationsPanel.i18n";

type Messages = (typeof recommendationsMessages)["vi"];

type Phase =
  | { kind: "idle" }
  | { kind: "running" }
  | { kind: "stopped" }
  | { kind: "failed"; message: string }
  | { kind: "done"; report: RiskReport; asked: Project };

interface RecommendationsPanelProps {
  project: Project;
  /** Whether this computer's license includes AI recommendations. */
  aiPro: boolean;
  /** Changes when the AI settings dialog closes, so the panel reads the settings again. */
  settingsKey: number;
  onOpenLicense: () => void;
  onOpenSettings: () => void;
  /** Select a task named in a card, on the Gantt. */
  onPickTask: (uid: number) => void;
  onClose: () => void;
}

const SEVERITY_LABEL: Record<RiskSeverity, keyof Messages> = {
  high: "severityHigh",
  medium: "severityMedium",
  low: "severityLow",
};

function formatCount(value: number): string {
  return new Intl.NumberFormat(getLang() === "en" ? "en-US" : "vi-VN").format(value);
}

/**
 * The Recommendations panel beside the Gantt (phase 22, PR C): recommendation kind 1,
 * explain risks. Read only. Without the `ai` license it introduces the feature; with AI
 * off it points to its settings. A request runs until it answers or Stop; closing the
 * panel or asking again stops the one running, and an answer that comes after is dropped.
 */
export function RecommendationsPanel(props: RecommendationsPanelProps) {
  const { project, aiPro, settingsKey, onOpenLicense, onOpenSettings, onPickTask, onClose } = props;
  const m = useMessages(recommendationsMessages);
  const [settings, setSettings] = useState<AiSettings | null>(null);
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });
  // Each request has a number; only the latest one may change the panel.
  const request = useRef(0);
  const running = phase.kind === "running";
  const names = useMemo(() => adviceNames(project), [project]);

  useEffect(() => {
    if (!aiPro) return;
    let live = true;
    aiSettings()
      .then((loaded) => live && setSettings(loaded))
      .catch(() => live && setSettings(null));
    return () => {
      live = false;
    };
  }, [aiPro, settingsKey]);

  // Closing the panel stops a request still running.
  const runningRef = useRef(false);
  runningRef.current = running;
  useEffect(
    () => () => {
      request.current += 1;
      if (runningRef.current) void aiCancel().catch(() => undefined);
    },
    [],
  );

  async function explain() {
    const id = ++request.current;
    const asked = project;
    setPhase({ kind: "running" });
    try {
      const report = await aiRisks();
      if (id !== request.current) return;
      setPhase(report ? { kind: "done", report, asked } : { kind: "stopped" });
    } catch (error) {
      if (id === request.current) setPhase({ kind: "failed", message: errorMessage(error) });
    }
  }

  function stop() {
    request.current += 1;
    setPhase({ kind: "stopped" });
    void aiCancel().catch(() => undefined);
  }

  return (
    <aside className="evm-panel ai-panel" aria-label={m.panel} aria-busy={running}>
      <div className="evm-head">
        <strong>
          {m.heading}
          <ProBadge />
        </strong>
        <Button Icon={X} onClick={onClose} aria-label={m.closePanel}>{m.close}</Button>
      </div>
      {!aiPro ? (
        <div className="evm-empty">
          <strong>{m.introTitle}</strong>
          <p className="ai-panel-text">{m.intro}</p>
          <Button variant="primary" Icon={KeyRound} onClick={onOpenLicense}>
            {m.activate}
          </Button>
        </div>
      ) : settings && !settings.enabled ? (
        <div className="evm-empty">
          <strong>{m.offTitle}</strong>
          <p className="ai-panel-text">{m.off}</p>
          <Button Icon={Settings2} onClick={onOpenSettings}>
            {m.openSettings}
          </Button>
        </div>
      ) : (
        <>
          <div className="ai-panel-actions">
            {running ? (
              <Button Icon={Square} onClick={stop}>
                {m.stop}
              </Button>
            ) : (
              <Button variant="primary" Icon={Sparkles} disabled={!settings} title={m.explainHint} onClick={() => void explain()}>
                {phase.kind === "done" ? m.explainAgain : m.explain}
              </Button>
            )}
          </div>
          <div role="status" className="evm-note">
            {running ? m.running : phase.kind === "stopped" ? m.stopped : phase.kind === "idle" ? m.explainHint : ""}
          </div>
          {phase.kind === "failed" && (
            <p role="alert" className="ai-panel-error">
              {m.failed(phase.message)}
            </p>
          )}
          {phase.kind === "done" && (
            <Results report={phase.report} changed={phase.asked !== project} names={names} m={m} onPickTask={onPickTask} />
          )}
        </>
      )}
    </aside>
  );
}

interface ResultsProps {
  report: RiskReport;
  changed: boolean;
  names: AdviceNames;
  m: Messages;
  onPickTask: (uid: number) => void;
}

function Results({ report, changed, names, m, onPickTask }: ResultsProps) {
  return (
    <>
      {changed && <p className="evm-note">{m.changed}</p>}
      {report.cards.length === 0 ? (
        <p className="evm-note">{m.none}</p>
      ) : (
        <ol className="ai-cards">
          {report.cards.map((card, at) => (
            <li key={at}>
              <Card card={card} names={names} m={m} onPickTask={onPickTask} />
            </li>
          ))}
        </ol>
      )}
      {report.truncated && <p className="evm-note">{m.truncated(report.omitted_tasks, report.omitted_resources)}</p>}
      <p className="evm-note ai-tokens">
        {m.tokens(formatCount(report.input_tokens), formatCount(report.output_tokens), report.requests)}
      </p>
    </>
  );
}

interface CardProps {
  card: RiskCard;
  names: AdviceNames;
  m: Messages;
  onPickTask: (uid: number) => void;
}

function Card({ card, names, m, onPickTask }: CardProps) {
  const text = (value: string) => <Text value={value} names={names} m={m} onPickTask={onPickTask} />;
  const chain = card.chain.map((ref) => refUid(ref, "T")).filter((uid): uid is number => uid != null && names.tasks.has(uid));
  const people = card.people
    .map((ref) => refUid(ref, "R"))
    .flatMap((uid) => (uid != null && names.resources.has(uid) ? [names.resources.get(uid) as string] : []));
  return (
    <article className={`ai-card ai-card-${card.severity}`}>
      <header className="ai-card-head">
        <span className={`ai-severity ai-severity-${card.severity}`}>{m[SEVERITY_LABEL[card.severity]] as string}</span>
        <h3>{text(card.title)}</h3>
      </header>
      <dl>
        <dt>{m.what}</dt>
        <dd>{text(card.what)}</dd>
        <dt>{m.why}</dt>
        <dd>{text(card.why)}</dd>
        {chain.length > 0 && (
          <>
            <dt>{m.chain}</dt>
            <dd>
              <ol className="ai-chain">
                {chain.map((uid) => (
                  <li key={uid}>
                    <TaskLink uid={uid} name={names.tasks.get(uid) as string} m={m} onPickTask={onPickTask} />
                  </li>
                ))}
              </ol>
            </dd>
          </>
        )}
        <dt>{m.focus}</dt>
        <dd>{text(card.focus)}</dd>
        {people.length > 0 && (
          <>
            <dt>{m.people}</dt>
            <dd>{people.join(", ")}</dd>
          </>
        )}
      </dl>
    </article>
  );
}

function TaskLink({ uid, name, m, onPickTask }: { uid: number; name: string; m: Messages; onPickTask: (uid: number) => void }) {
  return (
    <button type="button" className="ai-task-link" title={m.pickTask(name)} onClick={() => onPickTask(uid)}>
      {name}
    </button>
  );
}

/** A card's text with `T42` and `R7` shown as their names; tasks can be selected. */
function Text({ value, names, m, onPickTask }: { value: string; names: AdviceNames; m: Messages; onPickTask: (uid: number) => void }) {
  return (
    <>
      {adviceParts(value, names).map((part, at) =>
        part.kind === "task" ? (
          <TaskLink key={at} uid={part.uid} name={part.name} m={m} onPickTask={onPickTask} />
        ) : part.kind === "resource" ? (
          <strong key={at}>{part.name}</strong>
        ) : (
          <span key={at}>{part.text}</span>
        ),
      )}
    </>
  );
}
