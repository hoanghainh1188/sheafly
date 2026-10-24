import { ArrowDown, ArrowUp, Plus, X } from "lucide-react";
import { useReducer, useRef, useState } from "react";

import { formatUnits, hoursPerDay, parseUnits, showsUnits } from "../assignmentUnits";
import { formatFieldValue, parseFieldValue, valueKey } from "../fieldValues";
import { useMessages } from "../i18n";
import type { AssignmentRule, Command, Condition, FieldDef, FieldValue, Project, RuleTarget, Snapshot } from "../types";
import { usePendingList } from "../usePendingList";
import { rulesTabMessages } from "./AssignmentDialog.i18n";
import { Button } from "./Button";

type Send = (command: Command) => Promise<Snapshot | void>;
/** A change to a rule, made to the rule as it was last sent. */
type Update = (current: AssignmentRule) => AssignmentRule;
/** New values for a condition from its current ones; `null` removes the condition. */
type ValuesUpdate = (values: FieldValue[]) => FieldValue[] | null;

/** The engine's limits (`skills::MAX_RULES`, `skills::MAX_CONDITIONS`). */
const MAX_RULES = 200;
const MAX_CONDITIONS = 8;

/** Several text or number values are typed apart by semicolons: a comma is a decimal mark. */
const SEPARATOR = ";";

function targetKey(target: RuleTarget): string {
  return target.type === "resource" ? `resource:${target.uid}` : `skill:${target.id}`;
}

function targetOf(key: string): RuleTarget {
  const [type, id] = key.split(":");
  return type === "resource" ? { type: "resource", uid: Number(id) } : { type: "skill", id: Number(id) };
}

function formatValues(values: FieldValue[]): string {
  return values.map(formatFieldValue).join(`${SEPARATOR} `);
}

/** The values typed for `field`, each once; `null` when none is typed or one does not fit. */
export function parseValues(field: FieldDef, text: string): FieldValue[] | null {
  const pieces = text.split(SEPARATOR).map((piece) => piece.trim()).filter(Boolean);
  const values = new Map<string, FieldValue>();
  for (const piece of pieces) {
    const value = parseFieldValue(field, piece);
    if (value === undefined || value === null) return null;
    values.set(valueKey(value), value);
  }
  return values.size > 0 ? [...values.values()] : null;
}

/**
 * The ordered assignment rules. Every change sends the whole list, building on the list
 * sent last; Hủy of the dialog undoes them all.
 *
 * The cards show that list, not the one the engine last answered with, so a card's place
 * is where its rule is in the list a change builds on: a click before the engine answers
 * the last change still finds its rule. A refused change goes back to what the engine
 * holds, and the cards redraw from it.
 */
export function RulesTab({ project, send }: { project: Project; send: Send }) {
  const r = useMessages(rulesTabMessages);
  const [, redraw] = useReducer((count: number) => count + 1, 0);
  // Inputs keep what was typed until their key changes: a refused edit remounts the cards.
  const [refusals, refused] = useReducer((count: number) => count + 1, 0);
  const { latest, apply } = usePendingList(project.assignment_rules, async (rules) => {
    const state = await send({ type: "set_assignment_rules", rules });
    if (!state) {
      // The rules go back to what the engine holds; keys made for the refused order go too.
      ids.current = [];
      refused();
    }
    return state;
  });
  const rules = latest();
  // A key per rule that follows it when it moves, so focus and a half-made condition stay
  // with their rule. A list of another length (undo, a refused change) starts afresh.
  const ids = useRef<number[]>([]);
  const nextId = useRef(0);
  if (ids.current.length !== rules.length) ids.current = rules.map(() => nextId.current++);
  const firstTarget: RuleTarget | null =
    project.skills.length > 0
      ? { type: "skill", id: project.skills[0].id }
      : project.resources.length > 0
        ? { type: "resource", uid: project.resources[0].uid }
        : null;

  function sendList(next: AssignmentRule[], nextIds: number[]) {
    ids.current = nextIds;
    const sent = apply(next);
    redraw();
    void sent.finally(redraw);
  }

  /** Change the rule at `at` as it stands in the list sent last. */
  function change(at: number, update: Update) {
    const current = latest();
    if (at >= current.length) return;
    const next = update(current[at]);
    if (next !== current[at]) sendList(current.map((item, index) => (index === at ? next : item)), ids.current);
  }

  function move(at: number, by: -1 | 1) {
    const other = at + by;
    const current = latest();
    if (other < 0 || other >= current.length) return;
    const next = [...current];
    const nextIds = [...ids.current];
    [next[at], next[other]] = [next[other], next[at]];
    [nextIds[at], nextIds[other]] = [nextIds[other], nextIds[at]];
    sendList(next, nextIds);
  }

  function remove(at: number) {
    const current = latest();
    if (at < current.length) {
      sendList(
        current.filter((_, index) => index !== at),
        ids.current.filter((_, index) => index !== at),
      );
    }
  }

  function add() {
    const current = latest();
    if (firstTarget && current.length < MAX_RULES) {
      sendList([...current, { conditions: [], target: firstTarget, units: 1 }], [...ids.current, nextId.current++]);
    }
  }

  return (
    <>
      <p className="meta">{r.intro}</p>
      {project.fields.length === 0 && <p className="meta">{r.noFields}</p>}
      {rules.length === 0 ? (
        <p>{r.none}</p>
      ) : (
        <ol className="rule-list" role="list" aria-label={r.list}>
          {rules.map((rule, index) => (
            <RuleCard
              key={`${ids.current[index]}-${refusals}`}
              rule={rule}
              index={index}
              count={rules.length}
              project={project}
              onChange={(update) => change(index, update)}
              onMove={(by) => move(index, by)}
              onRemove={() => remove(index)}
            />
          ))}
        </ol>
      )}
      <div className="row">
        <Button
          Icon={Plus}
          disabled={firstTarget == null || rules.length >= MAX_RULES}
          title={firstTarget == null ? r.addNeedsTarget : undefined}
          onClick={add}
        >
          {r.add}
        </Button>
        <span className="meta">{r.limit(MAX_RULES, MAX_CONDITIONS)}</span>
      </div>
      <p className="meta">{r.note}</p>
    </>
  );
}

function RuleCard({
  rule,
  index,
  count,
  project,
  onChange,
  onMove,
  onRemove,
}: {
  rule: AssignmentRule;
  index: number;
  count: number;
  project: Project;
  onChange: (update: Update) => void;
  onMove: (by: -1 | 1) => void;
  onRemove: () => void;
}) {
  const r = useMessages(rulesTabMessages);
  const n = index + 1;
  const fieldOf = (id: number) => project.fields.find((field) => field.id === id);
  const setValues = (fieldId: number, update: ValuesUpdate) =>
    onChange((current) => ({
      ...current,
      conditions: current.conditions.flatMap((condition) => {
        if (condition.field !== fieldId) return [condition];
        const values = update(condition.values);
        return values ? [{ ...condition, values }] : [];
      }),
    }));
  const addCondition = (condition: Condition) =>
    onChange((current) =>
      current.conditions.length >= MAX_CONDITIONS || current.conditions.some((own) => own.field === condition.field)
        ? current
        : { ...current, conditions: [...current.conditions, condition] },
    );

  return (
    <li className="rule-card" aria-label={r.rule(n)}>
      <div className="row rule-head">
        <span className="rule-title">{r.rule(n)}</span>
        <Button Icon={ArrowUp} className="btn-icon" aria-label={r.moveUp(n)} disabled={index === 0} onClick={() => onMove(-1)} />
        <Button
          Icon={ArrowDown}
          className="btn-icon"
          aria-label={r.moveDown(n)}
          disabled={index === count - 1}
          onClick={() => onMove(1)}
        />
        <Button Icon={X} className="btn-icon" aria-label={r.remove(n)} onClick={onRemove} />
      </div>
      <div className="rule-when">
        {rule.conditions.length === 0 && (
          <div className="row rule-condition">
            <span className="rule-word">{r.when}</span>
            <span>{r.everyTask}</span>
          </div>
        )}
        {rule.conditions.map((condition, at) => {
          const field = fieldOf(condition.field);
          return field ? (
            <ConditionRow
              key={condition.field}
              first={at === 0}
              field={field}
              condition={condition}
              onChange={(update) => setValues(condition.field, update)}
            />
          ) : null;
        })}
        {project.fields.length > 0 && rule.conditions.length < MAX_CONDITIONS && (
          <ConditionAdder
            n={n}
            fields={project.fields.filter((field) => !rule.conditions.some((condition) => condition.field === field.id))}
            onAdd={addCondition}
          />
        )}
      </div>
      <div className="row rule-target">
        <span className="rule-word" aria-hidden="true">
          {r.assignTo}
        </span>
        <select
          aria-label={r.targetOf(n)}
          value={targetKey(rule.target)}
          onChange={(event) => {
            const target = targetOf(event.target.value);
            onChange((current) => ({ ...current, target }));
          }}
        >
          {project.skills.length > 0 && (
            <optgroup label={r.skills}>
              {project.skills.map((skill) => (
                <option key={skill.id} value={`skill:${skill.id}`}>
                  {r.withSkill(skill.name)}
                </option>
              ))}
            </optgroup>
          )}
          {project.resources.length > 0 && (
            <optgroup label={r.people}>
              {project.resources.map((resource) => (
                <option key={resource.uid} value={`resource:${resource.uid}`}>
                  {resource.name}
                </option>
              ))}
            </optgroup>
          )}
        </select>
        {rule.target.type === "skill" && (project.skill_levels ?? []).length > 0 && (
          <>
            <span className="rule-word" aria-hidden="true">
              {r.atLeast}
            </span>
            <select
              aria-label={r.minLevelOf(n)}
              value={rule.target.min_level ?? ""}
              onChange={(event) => {
                const value = event.target.value;
                onChange((current) =>
                  current.target.type === "skill"
                    ? { ...current, target: { type: "skill", id: current.target.id, min_level: value === "" ? null : Number(value) } }
                    : current,
                );
              }}
            >
              <option value="">{r.anyLevel}</option>
              {(project.skill_levels ?? []).map((level) => (
                <option key={level.id} value={level.id}>
                  {level.name}
                </option>
              ))}
            </select>
          </>
        )}
        <span className="rule-word">{r.units}</span>
        <span className="availability-units">
          <input
            key={rule.units}
            aria-label={r.unitsOf(n)}
            title={r.unitsTip}
            defaultValue={formatUnits(rule.units)}
            onBlur={(event) => {
              const text = event.target.value;
              const units = parseUnits(text);
              if (units == null || units <= 0 || units === rule.units) event.target.value = formatUnits(rule.units);
              else if (!showsUnits(text, rule.units)) onChange((current) => ({ ...current, units }));
            }}
          />
          <span className="rule-word">{hoursPerDay(rule.units)}</span>
        </span>
      </div>
    </li>
  );
}

/** One condition: a choice field ticks its choices, the others take typed values. */
function ConditionRow({
  first,
  field,
  condition,
  onChange,
}: {
  first: boolean;
  field: FieldDef;
  condition: Condition;
  onChange: (update: ValuesUpdate) => void;
}) {
  const r = useMessages(rulesTabMessages);
  const keys = condition.values.map(valueKey);
  return (
    <div className="row rule-condition">
      <span className="rule-word">{first ? r.when : r.and}</span>
      <span className="rule-field">{field.name}</span>
      <span className="rule-word">{r.oneOf}</span>
      {field.kind.type === "choice" ? (
        <span className="rule-choices" role="group" aria-label={r.valuesOf(field.name)}>
          {field.kind.options.map((option) => {
            const on = keys.includes(option);
            return (
              <label key={option} className="check">
                <input
                  type="checkbox"
                  aria-label={r.choiceOf(field.name, option)}
                  checked={on}
                  // The last value stays: a condition needs one; remove it instead.
                  disabled={on && keys.length === 1}
                  onChange={() =>
                    onChange((values) => {
                      if (!values.includes(option)) return [...values, option];
                      return values.length > 1 ? values.filter((value) => value !== option) : values;
                    })
                  }
                />
                {option}
              </label>
            );
          })}
        </span>
      ) : (
        <input
          key={formatValues(condition.values)}
          className="rule-values"
          aria-label={r.valuesOf(field.name)}
          title={r.valuesHint}
          defaultValue={formatValues(condition.values)}
          onBlur={(event) => {
            // Untouched text is not read again: a value with a semicolon would split.
            if (event.target.value === formatValues(condition.values)) return;
            const values = parseValues(field, event.target.value);
            if (values == null) event.target.value = formatValues(condition.values);
            else if (values.map(valueKey).join("\u0000") !== keys.join("\u0000")) onChange(() => values);
          }}
        />
      )}
      <Button Icon={X} className="btn-icon" aria-label={r.removeCondition(field.name)} onClick={() => onChange(() => null)} />
    </div>
  );
}

/** Pick a field not used yet and its first value(s), then add the condition. */
function ConditionAdder({
  n,
  fields,
  onAdd,
}: {
  n: number;
  fields: FieldDef[];
  onAdd: (condition: Condition) => void;
}) {
  const r = useMessages(rulesTabMessages);
  const [fieldId, setFieldId] = useState<number | null>(null);
  const [text, setText] = useState("");
  const field = fields.find((item) => item.id === fieldId) ?? fields[0];
  if (!field) return null;
  const choices = field.kind.type === "choice" ? field.kind.options : null;
  const values = choices ? (choices.includes(text) ? [text] : choices.slice(0, 1)) : parseValues(field, text);
  return (
    <div className="row rule-condition rule-adder">
      <select
        aria-label={r.newField(n)}
        value={field.id}
        onChange={(event) => {
          setFieldId(Number(event.target.value));
          setText("");
        }}
      >
        {fields.map((item) => (
          <option key={item.id} value={item.id}>
            {item.name}
          </option>
        ))}
      </select>
      {choices ? (
        <select aria-label={r.newValue(n)} value={values?.[0] ?? ""} onChange={(event) => setText(event.target.value)}>
          {choices.map((option) => (
            <option key={option} value={option}>
              {option}
            </option>
          ))}
        </select>
      ) : (
        <input
          className="rule-values"
          aria-label={r.newValue(n)}
          title={r.valuesHint}
          placeholder={r.valuesPlaceholder}
          value={text}
          onChange={(event) => setText(event.target.value)}
        />
      )}
      <Button
        Icon={Plus}
        disabled={values == null || values.length === 0}
        onClick={() => {
          if (values == null || values.length === 0) return;
          onAdd({ field: field.id, values });
          setFieldId(null);
          setText("");
        }}
      >
        {r.addCondition}
      </Button>
    </div>
  );
}
