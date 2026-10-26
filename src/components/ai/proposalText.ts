import { formatDuration, formatInstant } from "../../format";
import { pick } from "../../i18n";
import type { Command, Project, ProposalGoal, TaskType } from "../../types";
import { formatUnits } from "../../assignmentUnits";
import { proposalTextMessages } from "./proposalText.i18n";

/**
 * A proposal's commands as sentences with the names of tasks and people, never `T42` or a
 * UID (an assignment or a link is named by its two ends). `project` is the project the
 * proposal was tried on; a task or person it no longer has shows as its ID.
 */
export function describeCommand(command: Command, project: Project): string {
  const t = pick(proposalTextMessages);
  const task = (uid: number) => project.tasks.find((item) => item.uid === uid)?.name ?? `T${uid}`;
  const person = (uid: number) => project.resources.find((item) => item.uid === uid)?.name ?? `R${uid}`;
  const assignment = (uid: number) => project.assignments.find((item) => item.uid === uid);
  const kinds: Record<TaskType, string> = { fixed_units: t.fixedUnits, fixed_duration: t.fixedDuration, fixed_work: t.fixedWork };
  switch (command.type) {
    case "assign_resource":
      return t.assign(task(command.task_uid), person(command.resource_uid), formatUnits(command.units));
    case "update_assignment":
    case "remove_assignment": {
      const found = assignment(command.uid);
      const [on, who] = found ? [task(found.task_uid), person(found.resource_uid)] : [`#${command.uid}`, "?"];
      return command.type === "remove_assignment" ? t.removeAssignment(on, who) : t.updateAssignment(on, who);
    }
    case "set_work":
      return t.setWork(task(command.uid), formatDuration(command.minutes));
    case "set_duration":
      return t.setDuration(task(command.uid), formatDuration(command.minutes));
    case "set_task_type":
      return t.setTaskType(task(command.uid), kinds[command.task_type]);
    case "set_priority":
      return t.setPriority(task(command.uid), command.priority);
    case "add_dependency":
      return t.addDependency(task(command.predecessor_uid), task(command.successor_uid), command.kind.toUpperCase());
    case "remove_dependency": {
      const link = project.dependencies.find((item) => item.uid === command.uid);
      return link ? t.removeDependency(task(link.predecessor_uid), task(link.successor_uid)) : t.removeLink;
    }
    case "set_predecessors":
      return t.setPredecessors(task(command.uid), command.text);
    case "set_constraint":
      return t.setConstraint(task(command.uid), command.constraint_type.toUpperCase(), command.date == null ? "" : formatInstant(command.date));
    case "set_deadline":
      return command.date == null ? t.clearDeadline(task(command.uid)) : t.setDeadline(task(command.uid), formatInstant(command.date));
    case "split_task":
      return t.split(task(command.uid), formatDuration(command.at), formatDuration(command.gap));
    case "set_interrupts":
      return t.interrupts(task(command.uid), command.interrupts);
    case "set_leveling_delay":
      return t.levelingDelay(task(command.uid), formatDuration(command.minutes));
    case "set_work_range":
      return t.workRange(task(command.uid));
    case "apply_leveling":
      return t.leveling(command.delays.filter((delay) => delay.minutes > 0).length);
    default:
      return t.other(command.type);
  }
}

/** What a proposal sets out to improve, with names. */
export function describeGoal(goal: ProposalGoal, project: Project): string {
  const t = pick(proposalTextMessages);
  switch (goal.kind) {
    case "deadline":
      return t.goalDeadline(project.tasks.find((item) => item.uid === goal.task)?.name ?? `T${goal.task}`);
    case "overallocation":
      return t.goalOverallocation(project.resources.find((item) => item.uid === goal.resource)?.name ?? `R${goal.resource}`);
    case "finish":
      return t.goalFinish;
    case "estimate":
      return t.goalEstimate(project.tasks.find((item) => item.uid === goal.task)?.name ?? `T${goal.task}`);
  }
}
