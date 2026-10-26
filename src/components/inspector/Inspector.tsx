import { formatDuration, formatMoney, parseDuration } from "../../format";
import { useMessages } from "../../i18n";
import type { Accrual, Command, ConstraintType, Project, Task, TaskType } from "../../types";
import { valueOf } from "../../fieldValues";
import { DateTimeField } from "../DateTimeField";
import { FieldInput } from "../FieldInput";
import { MoneyInput } from "../MoneyInput";
import { AssignmentEditor } from "./AssignmentEditor";
import { EstimateGroup } from "./EstimateGroup";
import { PaymentsGroup } from "./PaymentsGroup";
import { canSplit, SplitSection } from "./SplitSection";
import { inspectorMessages } from "./Inspector.i18n";
import { useInspectorOpen } from "./useInspectorOpen";

/** Constraint types in menu order; each id is also its message key. */
const CONSTRAINTS: ConstraintType[] = ["asap", "alap", "snet", "snlt", "fnet", "fnlt", "mso", "mfo"];

interface Props {
  project: Project;
  task: Task;
  send: (command: Command) => void;
  onEditSeries: (uid: number) => void;
  onSplit: (uid: number) => void;
  onEditCalendar: (calendarId: number) => void;
  onCreateCalendar: () => void;
  /** Open Lịch thanh toán. */
  onOpenPayments: () => void;
}

/**
 * Properties of the selected task, grouped like Project's task information. Fields that
 * only make sense for a leaf (actuals, work, assignments, task type) are left out for a
 * summary task instead of being shown disabled.
 */
export function Inspector(props: Props) {
  const { task } = props;
  const [open, toggle] = useInspectorOpen();
  const m = useMessages(inspectorMessages);
  return (
    <section className="inspector" aria-label={m.region}>
      <header className="inspector-head">
        <button
          type="button"
          className="inspector-toggle"
          aria-expanded={open}
          aria-label={m.details}
          title={open ? m.collapse : m.expand}
          onClick={toggle}
        >
          {open ? "▾" : "▸"}
        </button>
        <h2>{task.name.trim() || m.untitled}</h2>
        <span className="meta">
          {m.ids(task.id, task.wbs)}
          {task.summary ? m.summaryTag : ""}
          {task.milestone ? m.milestoneTag : ""}
        </span>
        <span className="meta inspector-facts">
          {m.costs(formatMoney(task.cost), formatMoney(task.actual_cost))}
          {task.leveling_delay ? m.levelingDelay(formatDuration(task.leveling_delay)) : ""}
          {task.late ? m.lateTag : ""}
          {task.conflict ? m.conflictTag : ""}
        </span>
      </header>
      {open && (
        <div className="inspector-body">
          <SeriesGroup {...props} />
          <ScheduleGroup {...props} />
          {!task.summary && <TrackingGroup {...props} />}
          <EstimateGroup project={props.project} task={task} send={props.send} />
          <CostGroup {...props} />
          <PaymentsGroup project={props.project} task={task} onOpen={props.onOpenPayments} />
          {!task.summary && (
            <fieldset className="inspector-group assign-group">
              <legend>{m.resources}</legend>
              <AssignmentEditor project={props.project} task={task} send={props.send} />
            </fieldset>
          )}
          <FieldsGroup {...props} />
          <fieldset className="inspector-group notes-group">
            <legend>{m.notes}</legend>
            <input
              aria-label={m.notes}
              defaultValue={task.notes}
              key={`n-${task.uid}-${task.notes}`}
              onBlur={(event) => props.send({ type: "set_notes", uid: task.uid, notes: event.target.value })}
            />
          </fieldset>
        </div>
      )}
    </section>
  );
}

/** The project's custom fields, one row each; a summary task takes values too. */
function FieldsGroup({ project, task, send }: Props) {
  const m = useMessages(inspectorMessages);
  if (project.fields.length === 0) return null;
  return (
    <fieldset className="inspector-group fields-group">
      <legend>{m.fields}</legend>
      {project.fields.map((field) => (
        <div key={field.id}>
          <label htmlFor={`field-${field.id}`}>{field.name}</label>
          <FieldInput
            // Another task starts fresh: no text typed for the one before.
            key={task.uid}
            id={`field-${field.id}`}
            field={field}
            value={valueOf(task, field)}
            ariaLabel={field.name}
            onCommit={(value) => send({ type: "set_field_value", uid: task.uid, field_id: field.id, value })}
          />
        </div>
      ))}
    </fieldset>
  );
}

function SeriesGroup({ project, task, onEditSeries }: Props) {
  const m = useMessages(inspectorMessages);
  const series = task.series_uid != null ? project.tasks.find((item) => item.uid === task.series_uid) : null;
  if (!task.recurrence && !series) return null;
  return (
    <fieldset className="inspector-group">
      <legend>{m.recurring}</legend>
      <button type="button" onClick={() => onEditSeries(task.recurrence ? task.uid : (series?.uid ?? task.uid))}>
        {m.editSeries}
      </button>
      {series && (
        <p className="meta">
          {task.detached ? m.detached : m.occurrenceOf(series.name)}
        </p>
      )}
    </fieldset>
  );
}

function ScheduleGroup({ project, task, send, onEditCalendar, onCreateCalendar }: Props) {
  const m = useMessages(inspectorMessages);
  const needsDate = task.constraint_type !== "asap" && task.constraint_type !== "alap";
  return (
    <fieldset className="inspector-group">
      <legend>{m.schedule}</legend>
      <div>
        <label>{m.constraint}</label>
        <select
          aria-label={m.constraint}
          value={task.constraint_type}
          onChange={(event) =>
            send({
              type: "set_constraint",
              uid: task.uid,
              constraint_type: event.target.value as ConstraintType,
              date: task.constraint_date,
            })
          }
        >
          {CONSTRAINTS.map((id) => (
            <option key={id} value={id}>{m[id]}</option>
          ))}
        </select>
      </div>
      {needsDate && (
        <div>
          <label>{m.constraintDate}</label>
          <DateTimeField
            ariaLabel={m.constraintDate}
            value={task.constraint_date}
            onCommit={(date) => send({ type: "set_constraint", uid: task.uid, constraint_type: task.constraint_type, date })}
          />
        </div>
      )}
      <div>
        <label>{m.deadline}</label>
        <DateTimeField
          optional
          ariaLabel={m.deadline}
          value={task.deadline}
          onCommit={(date) => send({ type: "set_deadline", uid: task.uid, date })}
        />
      </div>
      {!task.summary && (
        <div>
          <label>{m.taskCalendar}</label>
          <select
            aria-label={m.taskCalendar}
            value={task.calendar_id ?? ""}
            onChange={(event) =>
              send({
                type: "set_task_calendar",
                uid: task.uid,
                calendar_id: event.target.value ? Number(event.target.value) : null,
              })
            }
          >
            <option value="">{m.projectCalendar}</option>
            {project.calendars.map((calendar) => (
              <option key={calendar.id} value={calendar.id}>{calendar.name}</option>
            ))}
          </select>
          <div className="row">
            <button
              type="button"
              disabled={task.calendar_id == null}
              onClick={() => task.calendar_id != null && onEditCalendar(task.calendar_id)}
            >
              {m.editCalendar}
            </button>
            <button type="button" onClick={onCreateCalendar}>{m.newCalendar}</button>
          </div>
        </div>
      )}
      {!task.summary && (
        <div>
          <label>{m.taskType}</label>
          <select
            aria-label={m.taskType}
            value={task.task_type}
            onChange={(event) =>
              send({
                type: "set_task_type",
                uid: task.uid,
                task_type: event.target.value as TaskType,
                effort_driven: task.effort_driven,
              })
            }
          >
            <option value="fixed_units">{m.fixedUnits}</option>
            <option value="fixed_duration">{m.fixedDuration}</option>
            <option value="fixed_work">{m.fixedWork}</option>
          </select>
          <label className="check">
            <input
              type="checkbox"
              checked={task.effort_driven}
              onChange={(event) =>
                send({
                  type: "set_task_type",
                  uid: task.uid,
                  task_type: task.task_type,
                  effort_driven: event.target.checked,
                })
              }
            />
            {m.effortDriven}
          </label>
        </div>
      )}
      {!task.summary && !task.milestone && (
        <div>
          <label className="check">
            <input
              type="checkbox"
              checked={task.interrupts}
              aria-describedby={`interrupts-note-${task.uid}`}
              onChange={(event) => send({ type: "set_interrupts", uid: task.uid, interrupts: event.target.checked })}
            />
            {m.interrupts}
          </label>
          <p className="check-note" id={`interrupts-note-${task.uid}`}>{m.interruptsNote}</p>
        </div>
      )}
      <label className="check">
        <input
          type="checkbox"
          checked={task.manual}
          onChange={(event) =>
            send({ type: "set_manual", uid: task.uid, manual: event.target.checked, start: task.start, finish: task.finish })
          }
        />
        {m.manual}
      </label>
      <div>
        <label>{m.priority}</label>
        <input
          aria-label={m.priority}
          className="narrow"
          defaultValue={task.priority}
          key={`p-${task.uid}-${task.priority}`}
          onBlur={(event) => send({ type: "set_priority", uid: task.uid, priority: Number(event.target.value) || 0 })}
        />
      </div>
    </fieldset>
  );
}

function TrackingGroup({ project, task, send, onSplit }: Props) {
  const m = useMessages(inspectorMessages);
  const overtime = project.assignments
    .filter((item) => item.task_uid === task.uid)
    .reduce((sum, item) => sum + item.overtime_work, 0);
  return (
    <fieldset className="inspector-group">
      <legend>{m.tracking}</legend>
      <div>
        <label>{m.actualStart}</label>
        <DateTimeField
          optional
          ariaLabel={m.actualStart}
          value={task.actual_start}
          onCommit={(start) => send({ type: "set_actual_start", uid: task.uid, start })}
        />
      </div>
      <div>
        <label>{m.actualFinish}</label>
        <DateTimeField
          optional
          ariaLabel={m.actualFinish}
          value={task.actual_finish}
          onCommit={(finish) => send({ type: "set_actual_finish", uid: task.uid, finish })}
        />
      </div>
      <DurationInput
        label={m.work}
        name="work"
        task={task}
        value={task.work}
        onCommit={(minutes) => send({ type: "set_work", uid: task.uid, minutes })}
      />
      <DurationInput
        label={m.remainingWork}
        name="rw"
        task={task}
        value={task.remaining_work}
        onCommit={(minutes) => send({ type: "set_remaining_work", uid: task.uid, minutes })}
      />
      <DurationInput
        label={m.overtimeWork}
        name="ot"
        task={task}
        value={overtime}
        onCommit={(minutes) => send({ type: "set_overtime_work", uid: task.uid, minutes })}
      />
      {canSplit(task) && <SplitSection task={task} send={send} onSplit={onSplit} />}
    </fieldset>
  );
}

function DurationInput({
  label,
  name,
  task,
  value,
  onCommit,
}: {
  label: string;
  name: string;
  task: Task;
  value: number;
  onCommit: (minutes: number) => void;
}) {
  return (
    <div>
      <label>{label}</label>
      <input
        aria-label={label}
        className="narrow"
        defaultValue={formatDuration(value)}
        key={`${name}-${task.uid}-${value}`}
        onBlur={(event) => {
          const minutes = parseDuration(event.target.value);
          if (minutes !== null && minutes !== value) onCommit(minutes);
        }}
      />
    </div>
  );
}

function CostGroup({ project, task, send }: Props) {
  const editable = project.editable_actual_costs && !task.summary;
  const m = useMessages(inspectorMessages);
  return (
    <fieldset className="inspector-group">
      <legend>{m.cost}</legend>
      <div>
        <label>{m.fixedCost}</label>
        <MoneyInput
          ariaLabel={m.fixedCost}
          className="narrow"
          value={task.fixed_cost}
          onCommit={(cost) =>
            send({ type: "set_fixed_cost", uid: task.uid, cost: cost ?? 0, accrual: task.fixed_cost_accrual })
          }
        />
      </div>
      <div>
        <label title={m.actualCostTip}>{m.actualCost}</label>
        <MoneyInput
          ariaLabel={m.actualCost}
          className="narrow"
          title={m.actualCostTip}
          disabled={!editable}
          optional
          // Empty goes back to the computed cost, shown as the placeholder.
          value={editable ? (task.manual_actual_cost ?? null) : Math.round(task.actual_cost)}
          placeholder={editable ? formatMoney(task.actual_cost) : undefined}
          onCommit={(cost) => send({ type: "set_task_actual_cost", uid: task.uid, cost })}
        />
      </div>
      <div>
        <label>{m.accrual}</label>
        <select
          aria-label={m.accrual}
          value={task.fixed_cost_accrual}
          onChange={(event) =>
            send({
              type: "set_fixed_cost",
              uid: task.uid,
              cost: task.fixed_cost,
              accrual: event.target.value as Accrual,
            })
          }
        >
          <option value="start">{m.accrualStart}</option>
          <option value="prorated">{m.accrualProrated}</option>
          <option value="end">{m.accrualEnd}</option>
        </select>
      </div>
    </fieldset>
  );
}
