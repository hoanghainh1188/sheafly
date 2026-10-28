/** Input types that do not take typed text. */
const NON_TEXT_INPUTS = new Set(["button", "checkbox", "color", "file", "image", "radio", "range", "reset", "submit"]);

/** Menu action for a Cmd/Ctrl keyboard shortcut, or null when the key is not one. */
export function shortcutId(event: KeyboardEvent): string | null {
  if (!(event.metaKey || event.ctrlKey)) return null;
  const key = event.key.toLowerCase();
  if (key === "z") return event.shiftKey ? "redo" : "undo";
  if (key === "s") return event.shiftKey ? "save-as" : "save";
  if (key === "o") return "open";
  if (key === "n") return "new";
  if (key === "]") return "indent";
  if (key === "[") return "outdent";
  if (key === "backspace") return "delete";
  if (key === "d" && !event.shiftKey && !event.altKey) return "fill-down";
  if (key === "=" || key === "+") return "zoom-in";
  if (key === "-") return "zoom-out";
  if (key === "0") return "zoom-reset";
  // Option changes the key on macOS (Option+C is "ç"), so read the physical key.
  if (event.altKey && event.code === "KeyC") return "copy-task";
  if (event.altKey && event.code === "KeyV") return "paste-task";
  return null;
}

export function isEditableTarget(target: EventTarget | null): target is HTMLElement {
  if (!(target instanceof HTMLElement)) return false;
  // A select has no undo of its own, so Cmd+Z there should still undo the project.
  if (target instanceof HTMLTextAreaElement) return true;
  if (target instanceof HTMLInputElement) return !NON_TEXT_INPUTS.has(target.type);
  return target.isContentEditable || target.closest('[contenteditable]:not([contenteditable="false"])') !== null;
}

function fieldText(field: HTMLElement): string {
  if (field instanceof HTMLInputElement || field instanceof HTMLTextAreaElement) return field.value;
  return field.textContent ?? "";
}

/** Text each field held when it got focus or last committed (change event or Enter). */
const committed = new WeakMap<HTMLElement, string>();

function remember(event: Event): void {
  if (isEditableTarget(event.target)) committed.set(event.target, fieldText(event.target));
}

function rememberOnEnter(event: KeyboardEvent): void {
  if (event.key === "Enter") remember(event);
}

/** Start tracking committed field text; returns the cleanup. */
export function trackCommittedText(root: Document): () => void {
  root.addEventListener("focusin", remember, true);
  root.addEventListener("change", remember, true);
  root.addEventListener("keydown", rememberOnEnter, true);
  return () => {
    root.removeEventListener("focusin", remember, true);
    root.removeEventListener("change", remember, true);
    root.removeEventListener("keydown", rememberOnEnter, true);
  };
}

/** Whether the field holds typing that has not been committed to the project yet. */
export function hasUncommittedText(field: HTMLElement): boolean {
  const before = committed.get(field);
  return before !== undefined && before !== fieldText(field);
}

/**
 * Actions still allowed while a modal dialog covers the window: dialogs edit live, so
 * undo and redo show their effect there, saving changes nothing on screen, and a language
 * switch only relabels. Anything else (delete, indent, a view switch, opening a file,
 * another dialog) waits.
 */
const MODAL_ACTIONS = new Set([
  "undo",
  "redo",
  "save",
  "save-as",
  "lang-vi",
  "lang-en",
  "lang-ja",
  "theme-system",
  "theme-light",
  "theme-dark",
]);

/**
 * Whether a menu or shortcut action should act on the project. While a text field has
 * uncommitted typing, undo and redo belong to that field; Cmd+Backspace inside a field
 * always edits the text and never deletes the selected task.
 */
export function shouldRunAction(id: string, focused: EventTarget | null): boolean {
  if (!MODAL_ACTIONS.has(id) && document.querySelector('[aria-modal="true"]')) return false;
  if (!isEditableTarget(focused)) return true;
  if (id === "delete") return false;
  if (id === "undo" || id === "redo") return !hasUncommittedText(focused);
  return true;
}

/**
 * Fields commit their text on blur. Saving first blurs the focused field, so its
 * command joins the queue before the save, which waits for queued edits.
 */
export function commitFocusedField(): void {
  const active = document.activeElement;
  if (isEditableTarget(active)) active.blur();
}

/** Range Cmd+Backspace removes: the selection, or back to the start of the line. */
export function lineStartRange(value: string, start: number, end: number): [number, number] {
  if (start !== end) return [start, end];
  return [value.lastIndexOf("\n", start - 1) + 1, start];
}

/**
 * Do in the focused field what its key would have done there. Menu accelerators take
 * the key before the page sees it, so the field never gets its native undo or delete.
 * `execCommand` keeps the browser's undo stack and fires `input` for React.
 */
export function runFieldAction(id: string, field: HTMLElement): void {
  if (id === "undo" || id === "redo") {
    document.execCommand(id);
    return;
  }
  if (id !== "delete") return;
  if (field instanceof HTMLInputElement || field instanceof HTMLTextAreaElement) {
    const [from, to] = lineStartRange(field.value, field.selectionStart ?? 0, field.selectionEnd ?? 0);
    if (from === to) return;
    field.setSelectionRange(from, to);
  }
  document.execCommand("delete");
}
