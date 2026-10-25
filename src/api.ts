import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import { defineMessages, isLang, pick, type Lang } from "./i18n";
import { SnapshotCache, type FullSnapshot, type SnapshotAnswer } from "./snapshotPatch";
import { isThemeChoice, type ThemeChoice } from "./theme";
import type {
  AssignmentMatrix,
  CurveBucket,
  CurveFilter,
  CurveMeasure,
  CurvesReport,
  Forecast,
  Command,
  ImportProblem,
  ImportResource,
  ImportSheet,
  ImportTask,
  LicenseView,
  LoadBucket,
  LoadGroup,
  LoadMap,
  NetworkLayout,
  NetworkOptions,
  PlanPreview,
  ProjectMetrics,
  PullChanges,
  PullPreview,
  PullProgress,
  PullTask,
  PushOptions,
  PushPreview,
  PushProgress,
  PushReport,
  Resolution,
  Source,
  Snapshot,
  Suggestion,
  TaskAssignment,
  TaskClipboard,
  TrackerKind,
} from "./types";

const apiMessages = defineMessages(
  { failed: "Không thực hiện được thao tác" },
  { failed: "The action could not be completed" },
);

export function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

// Edits, undo, redo, and reads of the open project run on the backend's thread pool,
// so two calls in flight could finish in either order. Each waits for the one before,
// which keeps the project in step with the keys the user pressed.
let documentQueue: Promise<unknown> = Promise.resolve();

function inOrder<T>(work: () => Promise<T>): Promise<T> {
  const result = documentQueue.then(work);
  documentQueue = result.catch(() => undefined);
  return result;
}

// File commands may show a dialog. They wait for queued edits so a save includes them,
// but later calls do not wait behind the dialog: the backend refuses edits while it is open.
function afterQueued<T>(work: () => Promise<T>): Promise<T> {
  return documentQueue.then(work);
}

export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return pick(apiMessages).failed;
}

/** The Sheafly Pro license of this computer. */
export function licenseStatus(): Promise<LicenseView> {
  return invoke<LicenseView>("license_status");
}

/**
 * Enter a license key. `ok` when it unlocks Pro on this build and was saved; otherwise the
 * view says why, and the key saved before (if any) stays.
 */
export async function licenseInstall(key: string): Promise<{ ok: boolean; view: LicenseView }> {
  try {
    return { ok: true, view: await invoke<LicenseView>("license_install", { key }) };
  } catch (refused) {
    if (isLicenseView(refused)) return { ok: false, view: refused };
    throw refused;
  }
}

function isLicenseView(value: unknown): value is LicenseView {
  return typeof value === "object" && value !== null && "state" in value && "build_date" in value;
}

/** Forget the key on this computer; Sheafly goes back to Free. */
export function licenseRemove(): Promise<LicenseView> {
  return invoke<LicenseView>("license_remove");
}

/** The language the user chose in the menu, or null to follow the system. */
export async function getLanguage(): Promise<Lang | null> {
  const saved = await invoke<string | null>("get_language");
  return isLang(saved) ? saved : null;
}

/**
 * Tell the backend which language to use for the native menu, dialogs and error
 * messages. `remember` stores it as the user's choice; a language followed from the
 * system is not stored, so a later system change still applies.
 */
export function setLanguage(language: Lang, remember: boolean): Promise<void> {
  return invoke<void>("set_language", { language, remember });
}

/** The theme the user chose in View → Appearance; "system" when there is none. */
export async function getTheme(): Promise<ThemeChoice> {
  const saved = await invoke<string>("get_theme");
  return isThemeChoice(saved) ? saved : "system";
}

/** Remember the theme, check it in the native menu, and match the window to it. */
export function setTheme(theme: ThemeChoice): Promise<void> {
  return invoke<void>("set_theme", { theme });
}

// Edits, undo and redo answer with patches against the snapshot held here; every other
// answer that is a snapshot passes through it too, so a patch never lands on the wrong base.
const snapshots = new SnapshotCache();
const wholeState = () => invoke<FullSnapshot>("project_state");
const received = (answer: Promise<SnapshotAnswer>) => answer.then((value) => snapshots.receive(value, wholeState));

export function loadState(): Promise<Snapshot> {
  return inOrder(() => received(wholeState()));
}

export function loadMetrics(): Promise<ProjectMetrics> {
  return inOrder(() => invoke<ProjectMetrics>("project_metrics"));
}

export function loadNetworkLayout(options: NetworkOptions): Promise<NetworkLayout> {
  return inOrder(() => invoke<NetworkLayout>("network_layout", { options }));
}

/** Who the assignment rules would give each unassigned task to; a read, never logged. */
export function suggestAssignments(): Promise<Suggestion[]> {
  return inOrder(() => invoke<Suggestion[]>("suggest_assignments"));
}

/** Work and capacity by person, role or skill from `from` to `to` ("YYYY-MM-DD", both included). */
export function loadMap(
  from: string,
  to: string,
  bucket: LoadBucket,
  group?: LoadGroup,
  baseline?: number,
): Promise<LoadMap> {
  return inOrder(() => invoke<LoadMap>("load_map", { from, to, bucket, group, baseline }));
}

/** Cumulative and periodic S-curves for Cost, Work, or Progress. */
export function curves(
  measure: CurveMeasure,
  bucket: CurveBucket,
  filter?: CurveFilter,
  baseline?: number,
): Promise<CurvesReport> {
  return inOrder(() => invoke<CurvesReport>("curves", { measure, bucket, filter, baseline }));
}

/** The resource assignment matrix (by person or by role); a read, never logged. */
export function assignmentMatrix(group: boolean): Promise<AssignmentMatrix> {
  return inOrder(() => invoke<AssignmentMatrix>("assignment_matrix", { group }));
}

/**
 * The finish forecast of the project as it is now. Not queued behind edits (it takes about
 * a second on a large project); a newer call stops this one, which then answers null.
 */
export function forecast(iterations: number): Promise<Forecast | null> {
  return invoke<Forecast | null>("forecast", { iterations });
}

/** What assigning `assignments` and leveling would do; the project does not change. */
export function previewPlan(assignments: TaskAssignment[]): Promise<PlanPreview> {
  return inOrder(() => invoke<PlanPreview>("preview_plan", { assignments }));
}

/** Apply the plan last previewed as one undo step; refused when the project changed since. */
export function applyPlan(plan: PlanPreview): Promise<Snapshot> {
  return inOrder(() =>
    received(invoke<SnapshotAnswer>("apply_plan", { assignments: plan.assignments, delays: plan.delays, splits: plan.splits })),
  );
}

/** Ask for a CSV or workbook and read its first sheet; null when cancelled. */
export function pickTable(): Promise<ImportSheet | null> {
  return afterQueued(() => invoke<ImportSheet | null>("pick_table"));
}

/** Another sheet of the workbook `pickTable` read. */
export function readTableSheet(sheet: string): Promise<ImportSheet> {
  return invoke<ImportSheet>("read_table_sheet", { sheet });
}

/** What `import_rows` would refuse, row by row; empty when it would succeed. */
export function checkImport(afterUid: number | null, resources: ImportResource[], tasks: ImportTask[]): Promise<ImportProblem[]> {
  return inOrder(() => invoke<ImportProblem[]>("check_import", { afterUid, resources, tasks }));
}

/**
 * Save the credentials for a tracker's site in the operating system's credential store: the
 * API key or token, and for trackers that sign in with an email, that account.
 */
export function trackerSaveKey(kind: TrackerKind, site: string, key: string, account: string | null = null): Promise<void> {
  return invoke<void>("tracker_save_key", { kind, site, key, account });
}

/** Whether a key is saved for the site; the key itself is never sent back. */
export function trackerHasKey(kind: TrackerKind, site: string): Promise<boolean> {
  return invoke<boolean>("tracker_has_key", { kind, site });
}

export function trackerForgetKey(kind: TrackerKind, site: string): Promise<void> {
  return invoke<void>("tracker_forget_key", { kind, site });
}

/** Check the saved key and the project key against the tracker; the project's name. */
export function trackerTest(source: Source): Promise<string> {
  return invoke<string>("tracker_test", { source });
}

/**
 * Fetch the source's issues and say what pulling them would do. The project does not
 * change; `apply_pull` with the rows does that. Null when cancelled. Not queued behind edits.
 */
export function trackerPull(source: Source): Promise<PullPreview | null> {
  return invoke<PullPreview | null>("tracker_pull", { source });
}

/** What pulling `rows` would do to the project as it is now. */
export function trackerCheck(rows: PullTask[], pulledAt: number, resolutions: Resolution[] = []): Promise<PullChanges> {
  return inOrder(() => invoke<PullChanges>("tracker_check", { rows, pulledAt, resolutions }));
}

/** Stop the pull that is fetching. */
export function trackerCancel(): Promise<void> {
  return invoke<void>("tracker_cancel");
}

/** What a push would write to the source's issues; nothing is written. Null when cancelled. */
export function trackerPushPreview(source: Source, options: PushOptions): Promise<PushPreview | null> {
  return invoke<PushPreview | null>("tracker_push_preview", { source, options });
}

/** Write the plan last previewed; refused when the project or settings changed since. */
export function trackerPush(source: Source, options: PushOptions): Promise<PushReport> {
  return invoke<PushReport>("tracker_push", { source, options });
}

export function onTrackerPushProgress(handler: (progress: PushProgress) => void): Promise<UnlistenFn> {
  return listen<PushProgress>("tracker-push-progress", (event) => handler(event.payload));
}

export function onTrackerProgress(handler: (progress: PullProgress) => void): Promise<UnlistenFn> {
  return listen<PullProgress>("tracker-progress", (event) => handler(event.payload));
}

/** The tasks `uids` with their subtrees, ready for a `paste_tasks` command. */
export function copyTasks(uids: number[]): Promise<TaskClipboard> {
  return inOrder(() => invoke<TaskClipboard>("copy_tasks", { uids }));
}

export function applyCommand(command: Command): Promise<Snapshot> {
  return inOrder(() => received(invoke<SnapshotAnswer>("apply_command", { command })));
}

export function undo(): Promise<Snapshot> {
  return inOrder(() => received(invoke<SnapshotAnswer>("undo")));
}

export function redo(): Promise<Snapshot> {
  return inOrder(() => received(invoke<SnapshotAnswer>("redo")));
}

export function newProject(): Promise<Snapshot> {
  return afterQueued(() => received(invoke<SnapshotAnswer>("new_project")));
}

export function openProject(): Promise<Snapshot> {
  return afterQueued(() => received(invoke<SnapshotAnswer>("open_project")));
}

/** File → Open Recent → the file at `index` (0 is the most recent). */
export function openRecent(index: number): Promise<Snapshot> {
  return afterQueued(() => received(invoke<SnapshotAnswer>("open_recent", { index })));
}

/**
 * Open the next file the system asked to open (a double-clicked project). `null` when
 * nothing is queued or a dialog is up; the file then stays queued for a later call.
 */
export function openPending(): Promise<Snapshot | null> {
  return afterQueued(() => invoke<FullSnapshot | null>("open_pending").then((value) => (value ? snapshots.receive(value, wholeState) : null)));
}

/** The system asked to open a project file; call `openPending` when no dialog is up. */
export function onOpenFile(handler: () => void): Promise<UnlistenFn> {
  return listen("open-file", () => handler());
}

export function saveProject(): Promise<Snapshot> {
  return afterQueued(() => received(invoke<SnapshotAnswer>("save_project")));
}

export function saveProjectAs(): Promise<Snapshot> {
  return afterQueued(() => received(invoke<SnapshotAnswer>("save_project_as")));
}

export function importMspdi(): Promise<Snapshot> {
  return afterQueued(() => received(invoke<SnapshotAnswer>("import_mspdi")));
}

/** After a crash, offers the autosaved project back; the restored snapshot, or null. */
export function checkAutosave(): Promise<Snapshot | null> {
  return afterQueued(() => invoke<FullSnapshot | null>("check_autosave").then((value) => (value ? snapshots.receive(value, wholeState) : null)));
}

/** Asks, then folds old undo steps into the project's starting state. */
export function compactHistory(): Promise<Snapshot> {
  return afterQueued(() => received(invoke<SnapshotAnswer>("compact_history")));
}

export function exportMspdi(): Promise<void> {
  return afterQueued(() => invoke<void>("export_mspdi"));
}

export function exportExcel(): Promise<void> {
  return afterQueued(() => invoke<void>("export_excel"));
}

export function savePdf(bytes: Uint8Array, fileName: string): Promise<void> {
  return invoke<void>("save_pdf", { bytes: Array.from(bytes), fileName });
}

export function onMenu(handler: (id: string) => void): Promise<UnlistenFn> {
  return listen<string>("menu-action", (event) => handler(event.payload));
}
