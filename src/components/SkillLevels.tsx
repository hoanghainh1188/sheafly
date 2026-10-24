import { ArrowDown, ArrowUp, Plus, X } from "lucide-react";
import { useMemo, useState, type FormEvent } from "react";

import { useMessages } from "../i18n";
import type { Command, Project, SkillLevelInput, Snapshot } from "../types";
import { usePendingList } from "../usePendingList";
import { skillLevelsMessages } from "./AssignmentDialog.i18n";
import { Button } from "./Button";

type Send = (command: Command) => Promise<Snapshot | void>;

/** The engine's limits (`skill_levels::MAX_SKILL_LEVEL_NAME_CHARS`, `MAX_SKILL_LEVELS`). */
const MAX_NAME = 30;
const MAX_LEVELS = 20;

/** A level name the engine would refuse for its characters. */
export function badLevelName(name: string): boolean {
  return name.includes(",") || name.includes(":");
}

/**
 * The project's scale of skill levels, lowest first: rename, move, delete, add. Every
 * change sends the whole scale with `set_skill_levels`, each kept level by its id.
 */
export function SkillLevels({ project, send }: { project: Project; send: Send }) {
  const s = useMessages(skillLevelsMessages);
  const stored = useMemo<SkillLevelInput[]>(
    () => (project.skill_levels ?? []).map(({ id, name }) => ({ id, name })),
    [project.skill_levels],
  );
  const { latest, apply } = usePendingList(stored, (levels) => send({ type: "set_skill_levels", levels }));
  const [draft, setDraft] = useState("");
  const bad = badLevelName(draft);
  const levels = project.skill_levels ?? [];

  async function add(event: FormEvent) {
    event.preventDefault();
    const name = draft.trim();
    if (!name || bad) return;
    await apply([...latest(), { id: null, name }]);
    setDraft("");
  }

  function move(index: number, by: number) {
    const next = [...latest()];
    const [level] = next.splice(index, 1);
    next.splice(index + by, 0, level);
    void apply(next);
  }

  return (
    <section aria-labelledby="skill-levels-heading">
      <h3 id="skill-levels-heading">{s.heading}</h3>
      <p className="meta">{s.intro}</p>
      {levels.length > 0 && (
        <ol className="skill-list skill-levels" aria-label={s.list}>
          {levels.map((level, index) => (
            <li key={level.id}>
              <input
                key={level.name}
                aria-label={s.nameOf(level.name)}
                maxLength={MAX_NAME}
                defaultValue={level.name}
                onBlur={(event) => {
                  const input = event.target;
                  const name = input.value.trim();
                  if (!name || name === level.name || badLevelName(name)) {
                    input.value = level.name;
                    return;
                  }
                  void apply(latest().map((other) => (other.id === level.id ? { ...other, name } : other)));
                }}
              />
              <Button
                Icon={ArrowUp}
                className="btn-icon"
                aria-label={s.up(level.name)}
                disabled={index === 0}
                onClick={() => move(index, -1)}
              />
              <Button
                Icon={ArrowDown}
                className="btn-icon"
                aria-label={s.down(level.name)}
                disabled={index === levels.length - 1}
                onClick={() => move(index, 1)}
              />
              <Button
                Icon={X}
                className="btn-icon"
                aria-label={s.remove(level.name)}
                title={s.removeTip}
                onClick={() => void apply(latest().filter((other) => other.id !== level.id))}
              />
            </li>
          ))}
        </ol>
      )}
      {levels.length < MAX_LEVELS && (
        <form className="row" onSubmit={(event) => void add(event)}>
          <input
            aria-label={s.newName}
            placeholder={s.newPlaceholder}
            maxLength={MAX_NAME}
            value={draft}
            aria-invalid={bad}
            className={bad ? "bad" : undefined}
            title={bad ? s.badName : undefined}
            onChange={(event) => setDraft(event.target.value)}
          />
          <Button type="submit" Icon={Plus} disabled={!draft.trim() || bad}>
            {s.add}
          </Button>
        </form>
      )}
    </section>
  );
}
