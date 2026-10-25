import type { Project } from "../../types";

/** A piece of a recommendation's text: plain text, or a task or person named by ID. */
export type AdvicePart =
  | { kind: "text"; text: string }
  | { kind: "task"; uid: number; name: string }
  | { kind: "resource"; uid: number; name: string };

/** The names of a project's tasks and people by UID, built once per project. */
export interface AdviceNames {
  tasks: Map<number, string>;
  resources: Map<number, string>;
}

export function adviceNames(project: Project): AdviceNames {
  return {
    tasks: new Map(project.tasks.map((task) => [task.uid, task.name])),
    resources: new Map(project.resources.map((resource) => [resource.uid, resource.name])),
  };
}

const REF = /\b([TR])(\d+)\b/g;

/**
 * The AI names tasks `T42` and people `R7` (they are what it was sent, masked or not); this
 * splits its text so each one shows with its real name. An ID the project no longer has
 * (deleted since) stays as written.
 */
export function adviceParts(text: string, names: AdviceNames): AdvicePart[] {
  const parts: AdvicePart[] = [];
  let at = 0;
  for (const match of text.matchAll(REF)) {
    const uid = Number(match[2]);
    const name = match[1] === "T" ? names.tasks.get(uid) : names.resources.get(uid);
    if (name === undefined) continue;
    if (match.index > at) parts.push({ kind: "text", text: text.slice(at, match.index) });
    parts.push(match[1] === "T" ? { kind: "task", uid, name } : { kind: "resource", uid, name });
    at = match.index + match[0].length;
  }
  if (at < text.length) parts.push({ kind: "text", text: text.slice(at) });
  return parts;
}

/** `T42` → 42, `R7` → 7; null for anything else. */
export function refUid(ref: string, prefix: "T" | "R"): number | null {
  const match = /^([TR])(\d+)$/.exec(ref);
  return match && match[1] === prefix ? Number(match[2]) : null;
}
