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

/** `text` with every `T42` and `R7` the project has replaced by its name, for text that
 * leaves the app (a copied or saved report). */
export function withNames(text: string, names: AdviceNames): string {
  return adviceParts(text, names)
    .map((part) => (part.kind === "text" ? part.text : part.name))
    .join("");
}

/** One block of a status report's Markdown, as the panel draws it. */
export type ReportBlock =
  | { kind: "heading"; level: 1 | 2; text: string }
  | { kind: "list"; items: string[] }
  | { kind: "paragraph"; text: string };

/**
 * The simple Markdown the status report holds (headings `#`/`##`, bullet lists, paragraphs),
 * as blocks; `**bold**` loses its marks. Anything else stays text, so nothing the model
 * writes becomes markup.
 */
export function reportBlocks(markdown: string): ReportBlock[] {
  const blocks: ReportBlock[] = [];
  const plain = (text: string) => text.replace(/\*\*(.+?)\*\*/g, "$1");
  let paragraph: string[] = [];
  const flush = () => {
    if (paragraph.length > 0) blocks.push({ kind: "paragraph", text: plain(paragraph.join(" ")) });
    paragraph = [];
  };
  for (const raw of markdown.split("\n")) {
    const line = raw.trim();
    const heading = /^(#{1,6})\s+(.*)$/.exec(line);
    const item = /^[-*]\s+(.*)$/.exec(line);
    if (line === "") {
      flush();
    } else if (heading) {
      flush();
      blocks.push({ kind: "heading", level: heading[1].length === 1 ? 1 : 2, text: plain(heading[2]) });
    } else if (item) {
      flush();
      const last = blocks[blocks.length - 1];
      if (last?.kind === "list") last.items.push(plain(item[1]));
      else blocks.push({ kind: "list", items: [plain(item[1])] });
    } else {
      paragraph.push(line);
    }
  }
  flush();
  return blocks;
}
