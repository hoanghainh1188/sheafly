import { formatUnits, parseUnits, showsUnits } from "../assignmentUnits";
import { useMessages } from "../i18n";
import { useState } from "react";

import type { Calendar, Overallocation, Resource, ResourceKind, SkillDef, SkillLevel } from "../types";
import { formatFocus } from "../focusPercent";
import { FocusInput } from "./FocusInput";
import { MoneyInput } from "./MoneyInput";
import { resourceSheetMessages } from "./ResourceSheet.i18n";
import { ProBadge } from "./license/ProBadge";

/** The engine's limit (`skill_levels::MAX_GROUP_CHARS`). */
const MAX_GROUP = 100;

interface Props {
  resources: Resource[];
  calendars: Calendar[];
  /** The project's skills; each resource's are shown by name, edited in Giao việc. */
  skills: SkillDef[];
  /** The project's scale of skill levels; a skill with a level shows as “Skill (Level)”. */
  skillLevels?: SkillLevel[];
  overallocations: Overallocation[];
  /** The project's productivity factor, shown where a resource follows it. */
  focusFactor?: number;
  /** Whether Sheafly Pro unlocks own factors; without it a factor can only be cleared. */
  capacityPro?: boolean;
  onAdd: (kind: ResourceKind) => void;
  onChange: (resource: Resource) => void;
  /** Set (or, empty, clear) the email address that matches people at an external tracker. */
  onChangeEmail: (uid: number, email: string) => void;
  /** Set (or, empty, clear) the resource's role or team. */
  onChangeGroup: (uid: number, group: string) => void;
  /** Set (or, `null`, clear) the resource's own productivity factor. */
  onChangeFocus: (uid: number, focus: number | null) => void;
  onDelete: (uid: number) => void;
  onEditCalendar: (calendarId: number) => void;
  onCreateCalendar: (resource: Resource) => void;
  /** Open the days the resource has other units than its max units. */
  onEditAvailability: (uid: number) => void;
}

export function ResourceSheet({
  resources,
  calendars,
  skills,
  skillLevels = [],
  overallocations,
  focusFactor = 1,
  capacityPro = true,
  onAdd,
  onChange,
  onChangeEmail,
  onChangeGroup,
  onChangeFocus,
  onDelete,
  onEditCalendar,
  onCreateCalendar,
  onEditAvailability,
}: Props) {
  const m = useMessages(resourceSheetMessages);
  const overloaded = new Set(overallocations.map((item) => item.resource_uid));
  const [role, setRole] = useState<string | null>(null);
  const roles = [...new Set(resources.map((resource) => resource.group ?? "").filter(Boolean))].sort((a, b) =>
    a.localeCompare(b),
  );
  // A role filter whose role is gone shows everyone again.
  const shownRole = role != null && (role === "" || roles.includes(role)) ? role : null;
  const shown = shownRole == null ? resources : resources.filter((resource) => (resource.group ?? "") === shownRole);
  const skillNames = (resource: Resource) =>
    skills
      .filter((skill) => resource.skills.includes(skill.id))
      .map((skill) => {
        const levelId = resource.skill_levels?.[String(skill.id)];
        const level = skillLevels.find((known) => known.id === levelId);
        return level ? m.skillAt(skill.name, level.name) : skill.name;
      })
      .join(", ");
  return (
    <div className="resource-sheet">
      <div className="resource-tools">
        <button type="button" onClick={() => onAdd("person")}>{m.addPerson}</button>
        <button type="button" onClick={() => onAdd("machine")}>{m.addMachine}</button>
        {roles.length > 0 && (
          <label className="resource-role-filter">
            {m.roleFilter}
            <select
              value={shownRole == null ? "*" : shownRole}
              onChange={(event) => setRole(event.target.value === "*" ? null : event.target.value)}
            >
              <option value="*">{m.allRoles}</option>
              {roles.map((name) => (
                <option key={name} value={name}>
                  {name}
                </option>
              ))}
              <option value="">{m.noRole}</option>
            </select>
          </label>
        )}
      </div>
      <table>
        <thead>
          <tr>
            <th>{m.name}</th>
            <th>{m.kind}</th>
            <th title={m.roleTip}>{m.role}</th>
            <th>{m.maxUnits}</th>
            <th title={m.focusTip}>
              {m.focus}
              <ProBadge />
            </th>
            <th>{m.calendar}</th>
            <th>{m.standardRate}</th>
            <th>{m.overtimeRate}</th>
            <th>{m.costPerUse}</th>
            <th title={m.emailTip}>{m.email}</th>
            <th title={m.skillsTip}>{m.skills}</th>
            <th>{m.overallocated}</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          {shown.map((resource) => (
            <tr key={resource.uid} className={overloaded.has(resource.uid) ? "over" : ""}>
              <td>
                <input
                  key={`${resource.uid}-name`}
                  defaultValue={resource.name}
                  onBlur={(event) => {
                    if (event.target.value !== resource.name) onChange({ ...resource, name: event.target.value });
                  }}
                />
              </td>
              <td>
                <select
                  value={resource.kind}
                  onChange={(event) => onChange({ ...resource, kind: event.target.value as ResourceKind })}
                >
                  <option value="person">{m.person}</option>
                  <option value="machine">{m.machine}</option>
                </select>
              </td>
              <td>
                <input
                  key={`${resource.uid}-group-${resource.group ?? ""}`}
                  aria-label={m.roleOf(resource.name)}
                  maxLength={MAX_GROUP}
                  defaultValue={resource.group ?? ""}
                  onBlur={(event) => {
                    const group = event.target.value.trim();
                    if (group !== (resource.group ?? "")) onChangeGroup(resource.uid, group);
                  }}
                />
              </td>
              <td>
                <div className="units-cell">
                  <input
                    key={`${resource.uid}-units-${resource.max_units}`}
                    aria-label={m.maxUnitsOf(resource.name)}
                    title={m.maxUnitsTip}
                    defaultValue={formatUnits(resource.max_units)}
                    onBlur={(event) => {
                      const text = event.target.value;
                      const units = parseUnits(text);
                      if (units == null) event.target.value = formatUnits(resource.max_units);
                      else if (!showsUnits(text, resource.max_units)) onChange({ ...resource, max_units: units });
                    }}
                  />
                  {resource.availability.length > 0 && <span className="meta">{m.byDate}</span>}
                  <button
                    type="button"
                    aria-label={m.availabilityOf(resource.name)}
                    title={m.availabilityTip}
                    onClick={() => onEditAvailability(resource.uid)}
                  >
                    {m.availability}
                  </button>
                </div>
              </td>
              <td>
                <FocusInput
                  ariaLabel={m.focusOf(resource.name)}
                  value={resource.focus ?? null}
                  // A person follows the project's factor, a machine works all of its time.
                  placeholder={formatFocus(resource.kind === "person" ? focusFactor : 1)}
                  title={m.focusTip}
                  badTitle={m.focusBad}
                  allowEmpty
                  locked={!capacityPro}
                  lockedTitle={m.focusNeedsPro}
                  onCommit={(focus) => onChangeFocus(resource.uid, focus)}
                />
              </td>
              <td>
                <div className="calendar-cell">
                  <select
                    aria-label={m.calendarOf(resource.name)}
                    value={resource.calendar_id ?? ""}
                    onChange={(event) =>
                      onChange({
                        ...resource,
                        calendar_id: event.target.value ? Number(event.target.value) : null,
                      })
                    }
                  >
                    <option value="">{m.projectCalendar}</option>
                    {calendars.map((calendar) => (
                      <option key={calendar.id} value={calendar.id}>{calendar.name}</option>
                    ))}
                  </select>
                  <button
                    type="button"
                    disabled={resource.calendar_id == null}
                    onClick={() => resource.calendar_id != null && onEditCalendar(resource.calendar_id)}
                  >
                    {m.editCalendar}
                  </button>
                  <button type="button" onClick={() => onCreateCalendar(resource)}>{m.newCalendar}</button>
                </div>
              </td>
              <td>
                <MoneyInput
                  ariaLabel={`${m.standardRate}: ${resource.name}`}
                  value={resource.standard_rate}
                  onCommit={(amount) => onChange({ ...resource, standard_rate: amount ?? 0 })}
                />
              </td>
              <td>
                <MoneyInput
                  ariaLabel={`${m.overtimeRate}: ${resource.name}`}
                  value={resource.overtime_rate}
                  onCommit={(amount) => onChange({ ...resource, overtime_rate: amount ?? 0 })}
                />
              </td>
              <td>
                <MoneyInput
                  ariaLabel={`${m.costPerUse}: ${resource.name}`}
                  value={resource.cost_per_use}
                  onCommit={(amount) => onChange({ ...resource, cost_per_use: amount ?? 0 })}
                />
              </td>
              <td>
                <input
                  key={`${resource.uid}-email-${resource.email ?? ""}`}
                  type="email"
                  aria-label={m.emailOf(resource.name)}
                  defaultValue={resource.email ?? ""}
                  onBlur={(event) => {
                    const email = event.target.value.trim();
                    if (email !== (resource.email ?? "")) onChangeEmail(resource.uid, email);
                  }}
                />
              </td>
              <td className="skills-cell">{skillNames(resource)}</td>
              <td className={overloaded.has(resource.uid) ? "bad" : ""}>
                {overloaded.has(resource.uid) ? m.yes : m.no}
              </td>
              <td>
                <button type="button" onClick={() => onDelete(resource.uid)}>{m.remove}</button>
              </td>
            </tr>
          ))}
          {resources.length === 0 && (
            <tr>
              <td colSpan={13} className="empty">{m.empty}</td>
            </tr>
          )}
        </tbody>
      </table>
    </div>
  );
}
