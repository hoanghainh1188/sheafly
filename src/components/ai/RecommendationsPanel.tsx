import { KeyRound, Settings2, Sparkles, Square, Wrench, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import { aiCancel, aiFixes, aiRisks, aiSettings, errorMessage } from "../../api";
import { getLang, useMessages } from "../../i18n";
import type { AiSettings, FixReport, Project, RiskCard, RiskReport, RiskSeverity, Snapshot } from "../../types";
import { Button } from "../Button";
import { ProBadge } from "../license/ProBadge";
import { AdviceText, TaskLink } from "./AdviceLinks";
import { adviceNames, refUid, type AdviceNames } from "./adviceText";
import { Proposals } from "./Proposals";
import { recommendationsMessages } from "./RecommendationsPanel.i18n";

type Messages = (typeof recommendationsMessages)["vi"];

type Phase =
  | { kind: "idle" }
  | { kind: "running"; ask: "risks" | "fixes" }
  | { kind: "stopped" }
  | { kind: "failed"; message: string }
  | { kind: "done"; report: RiskReport; asked: Project }
  | { kind: "fixes"; report: FixReport };

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
  /** The project after the user applied a proposal. */
  onApplied: (snapshot: Snapshot) => void;
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
 * The Recommendations panel beside the Gantt (phase 22, PR C and D): kind 1, explain risks
 * (read only), and kind 2, proposals to fix overallocation and missed deadlines, which the
 * engine tries and only the user applies (`Proposals`). Without the `ai` license it introduces the feature; with AI
 * off it points to its settings. A request runs until it answers or Stop; closing the
 * panel or asking again stops the one running, and an answer that comes after is dropped.
 */
export function RecommendationsPanel(props: RecommendationsPanelProps) {
  const { project, aiPro, settingsKey, onOpenLicense, onOpenSettings, onPickTask, onApplied, onClose } = props;
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

  async function ask(kind: "risks" | "fixes") {
    const id = ++request.current;
    const asked = project;
    setPhase({ kind: "running", ask: kind });
    try {
      const next: Phase | null =
        kind === "risks"
          ? await aiRisks().then((report) => report && { kind: "done", report, asked })
          : await aiFixes().then((report) => report && { kind: "fixes", report });
      if (id !== request.current) return;
      setPhase(next ?? { kind: "stopped" });
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
              <>
                <Button variant="primary" Icon={Sparkles} disabled={!settings} title={m.explainHint} onClick={() => void ask("risks")}>
                  {phase.kind === "done" ? m.explainAgain : m.explain}
                </Button>
                <Button Icon={Wrench} disabled={!settings} title={m.fixHint} onClick={() => void ask("fixes")}>
                  {m.fix}
                </Button>
              </>
            )}
          </div>
          <div role="status" className="evm-note">
            {phase.kind === "running"
              ? phase.ask === "risks"
                ? m.running
                : m.runningFixes
              : phase.kind === "stopped"
                ? m.stopped
                : phase.kind === "idle"
                  ? m.idleHint
                  : ""}
          </div>
          {phase.kind === "failed" && (
            <p role="alert" className="ai-panel-error">
              {m.failed(phase.message)}
            </p>
          )}
          {phase.kind === "done" && (
            <Results report={phase.report} changed={phase.asked !== project} names={names} m={m} onPickTask={onPickTask} />
          )}
          {phase.kind === "fixes" && (
            <>
              <Proposals report={phase.report} project={project} names={names} onPickTask={onPickTask} onApplied={onApplied} />
              <Usage report={phase.report} m={m} />
            </>
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
      <Usage report={report} m={m} />
    </>
  );
}

/** Whether the summary sent was shortened, and the tokens a request took. */
function Usage({ report, m }: { report: RiskReport | FixReport; m: Messages }) {
  return (
    <>
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
  const text = (value: string) => <AdviceText value={value} names={names} onPickTask={onPickTask} />;
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
                    <TaskLink uid={uid} name={names.tasks.get(uid) as string} onPickTask={onPickTask} />
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
