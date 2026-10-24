import { Plus, X } from "lucide-react";
import { useMemo, useState, type FormEvent } from "react";

import { useMessages } from "../i18n";
import type { Command, Project, Resource, SkillDef, SkillLevel, Snapshot } from "../types";
import { usePendingList } from "../usePendingList";
import { skillsTabMessages } from "./AssignmentDialog.i18n";
import { Button } from "./Button";
import { SkillLevels } from "./SkillLevels";

type Send = (command: Command) => Promise<Snapshot | void>;

/** The engine's limit (`skills::MAX_SKILL_NAME_CHARS`). */
const MAX_NAME = 50;

/** What a person has in one skill: not the skill, the skill with no level, or a level id. */
type Held = { skill: number; level: number | null };

/**
 * The project's skills (add, rename, delete), its scale of levels, and who has which: a
 * grid of checkboxes, or of level selects once the project has a scale. Each change is its
 * own command; Hủy of the dialog undoes them all.
 */
export function SkillsTab({ project, send }: { project: Project; send: Send }) {
  const s = useMessages(skillsTabMessages);
  const [draft, setDraft] = useState("");
  const comma = draft.includes(",") || draft.includes(":");

  async function add(event: FormEvent) {
    event.preventDefault();
    const name = draft.trim();
    if (!name || comma) return;
    if (await send({ type: "add_skill", name })) setDraft("");
  }

  return (
    <>
      <p className="meta">{s.intro}</p>
      {project.skills.length === 0 ? (
        <p>{s.none}</p>
      ) : (
        <ul className="skill-list" role="list" aria-label={s.list}>
          {project.skills.map((skill) => (
            <SkillItem key={skill.id} skill={skill} send={send} />
          ))}
        </ul>
      )}
      <form className="row" onSubmit={(event) => void add(event)}>
        <input
          aria-label={s.newName}
          placeholder={s.newPlaceholder}
          maxLength={MAX_NAME}
          value={draft}
          aria-invalid={comma}
          className={comma ? "bad" : undefined}
          title={comma ? s.noComma : undefined}
          onChange={(event) => setDraft(event.target.value)}
        />
        <Button type="submit" Icon={Plus} disabled={!draft.trim() || comma}>
          {s.add}
        </Button>
      </form>
      <SkillLevels project={project} send={send} />
      {project.skills.length > 0 &&
        (project.resources.length === 0 ? (
          <p>{s.noPeople}</p>
        ) : (
          <div className="skill-grid-wrap">
            <table className="skill-grid" aria-label={s.people}>
              <thead>
                <tr>
                  <th scope="col">{s.person}</th>
                  {project.skills.map((skill) => (
                    <th key={skill.id} scope="col">
                      {skill.name}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {project.resources.map((resource) => (
                  <SkillRow
                    key={resource.uid}
                    resource={resource}
                    skills={project.skills}
                    levels={project.skill_levels ?? []}
                    send={send}
                  />
                ))}
              </tbody>
            </table>
          </div>
        ))}
    </>
  );
}

function SkillItem({ skill, send }: { skill: SkillDef; send: Send }) {
  const s = useMessages(skillsTabMessages);
  return (
    <li>
      <input
        // An undone rename shows the engine's name again.
        key={skill.name}
        aria-label={s.nameOf(skill.name)}
        maxLength={MAX_NAME}
        defaultValue={skill.name}
        onBlur={async (event) => {
          const input = event.target;
          const name = input.value.trim();
          if (!name || name === skill.name) {
            input.value = skill.name;
            return;
          }
          // A refused name (taken, or with a comma) goes back; the footer says why.
          if (!(await send({ type: "rename_skill", id: skill.id, name }))) input.value = skill.name;
        }}
      />
      <Button
        Icon={X}
        className="btn-icon"
        aria-label={s.remove(skill.name)}
        title={s.removeTip}
        onClick={() => void send({ type: "delete_skill", id: skill.id })}
      />
    </li>
  );
}

/** One person's skills and levels; quick changes build on the list sent last. */
function SkillRow({
  resource,
  skills,
  levels,
  send,
}: {
  resource: Resource;
  skills: SkillDef[];
  levels: SkillLevel[];
  send: Send;
}) {
  const s = useMessages(skillsTabMessages);
  const stored = useMemo<Held[]>(
    () => resource.skills.map((skill) => ({ skill, level: resource.skill_levels?.[String(skill)] ?? null })),
    [resource.skills, resource.skill_levels],
  );
  const { latest, apply } = usePendingList(stored, (next) => {
    const withLevels = next.filter((held) => held.level != null).map(({ skill, level }) => ({ skill, level: level! }));
    const skillIds = next.map((held) => held.skill);
    return send(
      withLevels.length > 0
        ? { type: "set_resource_skills", uid: resource.uid, skills: skillIds, levels: withLevels }
        : { type: "set_resource_skills", uid: resource.uid, skills: skillIds },
    );
  });
  /** Give the person `skill` at `level`, or (`undefined`) take it away. */
  const set = (skill: number, level: number | null | undefined) => {
    const others = latest().filter((held) => held.skill !== skill);
    const next = level === undefined ? others : [...others, { skill, level }];
    void apply(next.sort((a, b) => a.skill - b.skill));
  };
  return (
    <tr>
      <th scope="row">{resource.name}</th>
      {skills.map((skill) => {
        const held = stored.find((own) => own.skill === skill.id);
        return (
          <td key={skill.id}>
            {levels.length === 0 ? (
              <input
                type="checkbox"
                aria-label={s.has(resource.name, skill.name)}
                checked={held != null}
                onChange={() => set(skill.id, held ? undefined : null)}
              />
            ) : (
              <select
                aria-label={s.levelOf(resource.name, skill.name)}
                value={held == null ? "" : held.level == null ? "any" : String(held.level)}
                onChange={(event) => {
                  const value = event.target.value;
                  set(skill.id, value === "" ? undefined : value === "any" ? null : Number(value));
                }}
              >
                <option value="">{s.lacks}</option>
                <option value="any">{s.anyLevel}</option>
                {levels.map((level) => (
                  <option key={level.id} value={level.id}>
                    {level.name}
                  </option>
                ))}
              </select>
            )}
          </td>
        );
      })}
    </tr>
  );
}
