import { useMessages } from "../../i18n";
import { adviceParts, type AdviceNames } from "./adviceText";
import { recommendationsMessages } from "./RecommendationsPanel.i18n";

/** A task named in a recommendation: selects it on the Gantt. */
export function TaskLink({ uid, name, onPickTask }: { uid: number; name: string; onPickTask: (uid: number) => void }) {
  const m = useMessages(recommendationsMessages);
  return (
    <button type="button" className="ai-task-link" title={m.pickTask(name)} onClick={() => onPickTask(uid)}>
      {name}
    </button>
  );
}

/** A card's text with `T42` and `R7` shown as their names; tasks can be selected. */
export function AdviceText({ value, names, onPickTask }: { value: string; names: AdviceNames; onPickTask: (uid: number) => void }) {
  return (
    <>
      {adviceParts(value, names).map((part, at) =>
        part.kind === "task" ? (
          <TaskLink key={at} uid={part.uid} name={part.name} onPickTask={onPickTask} />
        ) : part.kind === "resource" ? (
          <strong key={at}>{part.name}</strong>
        ) : (
          <span key={at}>{part.text}</span>
        ),
      )}
    </>
  );
}
