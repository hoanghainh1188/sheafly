import { Check, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { aiApply, aiTrial, errorMessage } from "../../api";
import { formatHours, formatInstant } from "../../format";
import { useMessages } from "../../i18n";
import type { FixReport, Project, Proposal, ProposalMeasure, ProposalTrial, Snapshot } from "../../types";
import { Button } from "../Button";
import { PlanOutcome } from "../PlanOutcome";
import { AdviceText } from "./AdviceLinks";
import type { AdviceNames } from "./adviceText";
import { describeCommand, describeGoal } from "./proposalText";
import { proposalsMessages } from "./Proposals.i18n";

type Messages = (typeof proposalsMessages)["vi"];

/** The last trial of a proposal, for the project it ran on. */
type Trial = { project: Project; result: { ok: ProposalTrial } | { error: string } };

interface ProposalsProps {
  report: FixReport;
  project: Project;
  names: AdviceNames;
  onPickTask: (uid: number) => void;
  /** The project after a proposal was applied. */
  onApplied: (snapshot: Snapshot) => void;
}

/**
 * The proposals of kind 2 (phase 22 PR D): each tried by the engine on a copy of the project
 * as it is now, again whenever the project changes (decision D6), with what the trial found.
 * The ones that do not make their goal better, or that the engine refuses, are dropped,
 * without Apply. Nothing changes before Apply, which sends exactly the commands tried.
 */
export function Proposals({ report, project, names, onPickTask, onApplied }: ProposalsProps) {
  const m = useMessages(proposalsMessages);
  const [trials, setTrials] = useState<Map<string, Trial>>(new Map());
  const [gone, setGone] = useState<Set<string>>(new Set());
  const live = useMemo(() => report.proposals.filter((proposal) => !gone.has(proposal.id)), [report.proposals, gone]);

  useEffect(() => {
    let current = true;
    for (const proposal of live) {
      aiTrial(proposal)
        .then((ok) => ({ ok }), (error: unknown) => ({ error: errorMessage(error) }))
        .then((result) => {
          if (current) setTrials((old) => new Map(old).set(proposal.id, { project, result }));
        });
    }
    return () => {
      current = false;
    };
    // Not on `live`: a proposal leaving needs no new trials, a project change does.
  }, [project, report.proposals]);

  const leave = (id: string) => setGone((old) => new Set(old).add(id));
  const dropped = (proposal: Proposal) => {
    const result = trials.get(proposal.id)?.result;
    return result != null && ("error" in result || result.ok.verdict === "worse");
  };
  const kept = live.filter((proposal) => !dropped(proposal));
  const out = live.filter(dropped);
  const card = (proposal: Proposal) => (
    <li key={proposal.id}>
      <ProposalCard
        proposal={proposal}
        trial={trials.get(proposal.id)}
        project={project}
        names={names}
        m={m}
        onPickTask={onPickTask}
        onApplied={(snapshot) => {
          leave(proposal.id);
          onApplied(snapshot);
        }}
        onDismiss={() => leave(proposal.id)}
      />
    </li>
  );

  if (report.proposals.length === 0) {
    return <p className="evm-note">{report.note ? m.noneBecause(report.note) : m.none}</p>;
  }
  return (
    <>
      {kept.length > 0 && (
        <ol className="ai-cards" aria-label={m.proposals}>
          {kept.map(card)}
        </ol>
      )}
      {out.length > 0 && (
        <details className="ai-dropped">
          <summary>{m.dropped(out.length)}</summary>
          <ol className="ai-cards">{out.map(card)}</ol>
        </details>
      )}
    </>
  );
}

interface ProposalCardProps {
  proposal: Proposal;
  trial: Trial | undefined;
  project: Project;
  names: AdviceNames;
  m: Messages;
  onPickTask: (uid: number) => void;
  onApplied: (snapshot: Snapshot) => void;
  onDismiss: () => void;
}

function ProposalCard({ proposal, trial, project, names, m, onPickTask, onApplied, onDismiss }: ProposalCardProps) {
  const [applying, setApplying] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  // A trial for another state of the project is shown dimmed and cannot be applied.
  const fresh = trial?.project === project;
  const result = trial?.result;
  const ok = result && "ok" in result ? result.ok : null;
  const state = !result ? "trying" : ok ? ok.verdict : "refused";
  const label = { trying: m.trying, improved: m.improved, trade_off: m.tradeOff, worse: m.worse, refused: m.refused }[state];
  // What is applied: the commands tried, with the leveling the trial computed.
  const changes = ok?.commands ?? proposal.commands;

  async function apply() {
    if (!ok) return;
    setApplying(true);
    setFailure(null);
    try {
      onApplied(await aiApply(proposal.id, ok.commands));
    } catch (error) {
      setFailure(errorMessage(error));
      setApplying(false);
    }
  }

  return (
    <article className={`ai-card ai-proposal ai-proposal-${state}`} aria-busy={!fresh}>
      <header className="ai-card-head">
        <span className={`ai-verdict ai-verdict-${state}`}>{label}</span>
        <h3>{describeGoal(proposal.goal, project)}</h3>
      </header>
      <dl>
        <dt>{m.reason}</dt>
        <dd>
          <AdviceText value={proposal.reason} names={names} onPickTask={onPickTask} />
        </dd>
        <dt>{m.changes}</dt>
        <dd>
          <ul className="ai-changes">
            {changes.map((command, at) => (
              <li key={at}>{describeCommand(command, trial?.project ?? project)}</li>
            ))}
          </ul>
        </dd>
      </dl>
      {result && "error" in result && <p className="ai-panel-error">{m.refusedNote(result.error)}</p>}
      {ok && trial && <TrialOutcome trial={ok} project={trial.project} m={m} />}
      {failure && (
        <p role="alert" className="ai-panel-error">
          {m.applyFailed(failure)}
        </p>
      )}
      <div className="ai-proposal-actions">
        {ok && ok.verdict !== "worse" && (
          <Button
            variant="primary"
            Icon={Check}
            disabled={!fresh || applying}
            title={fresh ? m.applyHint : m.waitTrial}
            onClick={() => void apply()}
          >
            {applying ? m.applying : m.apply}
          </Button>
        )}
        <Button Icon={X} disabled={applying} onClick={onDismiss}>
          {m.dismiss}
        </Button>
      </div>
    </article>
  );
}

function TrialOutcome({ trial, project, m }: { trial: ProposalTrial; project: Project; m: Messages }) {
  const worse = new Set<ProposalMeasure>(trial.worse);
  const hours = (minutes: number) => formatHours(minutes / 60) || "0";
  const rows: [ProposalMeasure, string, string, string][] = [
    ["finish", m.finish, formatInstant(trial.before.finish), formatInstant(trial.after.finish)],
    ["late", m.late, String(trial.before.late), String(trial.after.late)],
    ["overallocation", m.over, hours(trial.before.over_minutes), hours(trial.after.over_minutes)],
    ["critical", m.critical, String(trial.before.critical), String(trial.after.critical)],
  ];
  return (
    <>
      {trial.verdict === "trade_off" && <p className="evm-note">{m.tradeOffNote}</p>}
      {trial.verdict === "worse" && <p className="evm-note">{m.worseNote}</p>}
      <table className="ai-measures" aria-label={m.outcome}>
        <thead>
          <tr>
            <th scope="col">{m.measure}</th>
            <th scope="col">{m.before}</th>
            <th scope="col">{m.after}</th>
          </tr>
        </thead>
        <tbody>
          {rows.map(([key, name, before, after]) => (
            <tr key={key}>
              <th scope="row">{name}</th>
              <td>{before}</td>
              <td className={worse.has(key) ? "bad" : undefined}>{after}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <PlanOutcome project={project} plan={trial} />
    </>
  );
}
