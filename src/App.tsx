import { tasksAsText, toggleHidden, visibleFields } from "./fieldColumns";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  applyCommand,
  checkAutosave,
  errorMessage,
  inTauri,
  licenseStatus,
  loadMetrics,
  loadNetworkLayout,
  loadState,
  suggestAssignments,
  pickTable,
  applyPlan,
  onMenu,
  onOpenFile,
  openPending,
  redo,
  savePdf,
  setTheme,
  undo,
} from "./api";
import { appMessages } from "./App.i18n";
import { linkAbove, resourceCommand } from "./appCommands";
import { BaselineDialog } from "./components/BaselineDialog";
import { LicenseDialog } from "./components/license/LicenseDialog";
import { AiSettingsDialog } from "./components/ai/AiSettingsDialog";
import { RecommendationsPanel } from "./components/ai/RecommendationsPanel";
import { WbsDialog } from "./components/ai/WbsDialog";
import { HealthDialog } from "./components/health/HealthDialog";
import { useScheduleHealth } from "./components/health/useScheduleHealth";
import { EvmPanel } from "./components/EvmPanel";
import { GanttChart } from "./components/GanttChart";
import { Inspector } from "./components/inspector/Inspector";
import { NetworkDiagram } from "./components/NetworkDiagram";
import { ProjectDialog, type ProjectTab } from "./components/ProjectDialog";
import { WorkingTimeDialog } from "./components/WorkingTimeDialog";
import { RecurringDialog, recurringCommand } from "./components/RecurringDialog";
import { ResourceAvailabilityDialog } from "./components/ResourceAvailabilityDialog";
import { AssignmentDialog } from "./components/AssignmentDialog";
import { AssignPreviewDialog } from "./components/AssignPreviewDialog";
import { ImportDialog } from "./components/ImportDialog";
import { TrackerDialog } from "./components/tracker/TrackerDialog";
import { TrackerPushDialog } from "./components/tracker/TrackerPushDialog";
import { TrackerSettingsDialog } from "./components/tracker/TrackerSettingsDialog";
import { ResourceSheet } from "./components/ResourceSheet";
import { AssignmentMatrixView } from "./components/matrix/AssignmentMatrixView";
import { SplitDialog } from "./components/SplitDialog";
import { StatusBar } from "./components/StatusBar";
import { TaskEditDialog } from "./components/TaskEditDialog";
import { stepZoom } from "./ganttZoom";
import { ForecastView } from "./components/forecast/ForecastView";
import { useForecast } from "./components/forecast/useForecast";
import { PaneSplitter } from "./components/split/PaneSplitter";
import { readShare, saveShare } from "./components/split/sheetShare";
import { TaskSheet } from "./components/TaskSheet";
import { TaskUsage } from "./components/TaskUsage";
import { LoadMapView } from "./components/load/LoadMapView";
import { CurvesView } from "./components/curves/CurvesView";
import { CashFlowView } from "./components/cashflow/CashFlowView";
import { VarianceAuditView } from "./components/diff/VarianceAuditView";
import { GitCompare, X } from "lucide-react";
import { PaymentScheduleDialog } from "./components/payments/PaymentScheduleDialog";
import { Toolbar } from "./components/Toolbar";
import { VIEW_PANEL_ID, ViewBar, viewTabId } from "./components/ViewBar";
import { instantToDateKey, normalizePredecessorText, todayInstant, utcMidnight } from "./format";
import { pick, useLang, useMessages, useSetLang } from "./i18n";
import { buildGanttPdf, pdfSuggestedName } from "./ganttPdf";
import { handleMenuAction } from "./menuActions";
import { buildNetworkPdf } from "./networkPdf";
import type { CashFlowOptions, Command, FillField, ImportSheet, LicenseView, NetworkLayout, PlanPreview, ProjectDiffReport, ProjectMetrics, Scale, Snapshot, Suggestion, TaskClipboard, ViewId } from "./types";
import {
  commitFocusedField,
  isEditableTarget,
  runFieldAction,
  shortcutId,
  shouldRunAction,
  trackCommittedText,
} from "./shortcuts";
import { applyTheme, themeChoice, type ThemeChoice } from "./theme";
import { linkFromAbove, toolbarStates } from "./toolbarState";
import { undoTo } from "./undoTo";
import { useFileDialog } from "./useFileDialog";
import { distinctValues, valueKey } from "./fieldValues";
import { lastFinish, spanEndInstant, type SpanEnd } from "./spanFit";
import { buildViewRows, fieldOfMode, healthCriterionOf, type FilterMode, type GroupMode, type HighlightMode } from "./viewRows";
import { windowTitle } from "./windowTitle";
import { selectionRange } from "./taskSelection";
import { parsePastedTable } from "./pastedTable";
import { useClipboardEvents } from "./useClipboardEvents";
import "./styles.css";

export default function App() {
  const m = useMessages(appMessages);
  const setLang = useSetLang();
  const lang = useLang();
  const [snap, setSnap] = useState<Snapshot | null>(null);
  const [view, setView] = useState<ViewId>("gantt");
  const [scale, setScaleState] = useState<Scale>("day");
  // How much wider a day column is than the scale's own; a scale choice starts it at 1.
  const [zoom, setZoom] = useState(1);
  const setScale = useCallback((next: Scale) => {
    setScaleState(next);
    setZoom(1);
  }, []);
  const zoomTimeline = useCallback(
    (direction: 1 | -1 | 0) => {
      // Only the Gantt views have a timeline; elsewhere the keys do nothing.
      if (view !== "gantt" && view !== "tracking") return;
      setZoom((current) => (direction === 0 ? 1 : stepZoom(scale, current, direction)));
    },
    [scale, view],
  );
  const [selected, setSelected] = useState<number | null>(null);
  // The other end of a Shift+click range in the sheet; null when one row is selected.
  const [selectionEnd, setSelectionEnd] = useState<number | null>(null);
  const [clipboard, setClipboard] = useState<TaskClipboard | null>(null);
  const [fillField, setFillField] = useState<FillField | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Thông tin dự án is a transaction: the history cursor when it opened, so Hủy can
  // undo back to it. null while the dialog is closed.
  const [infoStart, setInfoStart] = useState<number | null>(null);
  const [infoTab, setInfoTab] = useState<ProjectTab>("general");
  const [baselineOpen, setBaselineOpen] = useState(false);
  const [licenseOpen, setLicenseOpen] = useState(false);
  const [aiSettingsOpen, setAiSettingsOpen] = useState(false);
  const [healthOpen, setHealthOpen] = useState(false);
  // What this computer's license unlocks; the backend checks again on every gated command.
  const [license, setLicense] = useState<LicenseView | null>(null);
  useEffect(() => {
    if (inTauri()) void licenseStatus().then(setLicense, () => setLicense(null));
  }, []);
  const capacityPro = license?.features.includes("capacity") ?? false;
  const reportingPro = license?.features.includes("reporting") ?? false;
  const healthPro = license?.features.includes("health") ?? false;
  const cashflowPro = license?.features.includes("cashflow") ?? false;
  const diffPro = license?.features.includes("diff") ?? false;
  const [activeDiffReport, setActiveDiffReport] = useState<ProjectDiffReport | null>(null);
  // What the Dòng tiền view shows, so File → Export → Cash Flow exports the same.
  const [cashFlowOptions, setCashFlowOptions] = useState<CashFlowOptions>({ bucket: "month", baseline: null });
  // The calendar Lịch làm việc opened on and the history cursor to roll back to.
  const [workingTime, setWorkingTime] = useState<{ id: number; start: number } | null>(null);
  // The resource Khả dụng is open for and the history cursor to roll back to.
  const [availability, setAvailability] = useState<{ uid: number; start: number } | null>(null);
  // Giao việc (skills and assignment rules): the history cursor to roll back to.
  const [assignmentStart, setAssignmentStart] = useState<number | null>(null);
  // Lịch thanh toán: the history cursor to roll back to.
  const [paymentsStart, setPaymentsStart] = useState<number | null>(null);
  // Giao việc theo quy tắc: the suggestions being reviewed, null while closed.
  const [suggestions, setSuggestions] = useState<Suggestion[] | null>(null);
  const [importSheet, setImportSheet] = useState<ImportSheet | null>(null);
  // Kéo từ task tracker is open.
  const [trackerOpen, setTrackerOpen] = useState(false);
  // Đẩy ngày về task tracker is open.
  const [trackerPushOpen, setTrackerPushOpen] = useState(false);
  // Đồng bộ task tracker (settings) is open.
  const [trackerSettingsOpen, setTrackerSettingsOpen] = useState(false);
  const [rowFilter, setRowFilter] = useState<FilterMode>("none");
  const [filterResource, setFilterResource] = useState<number | null>(null);
  const [filterValue, setFilterValue] = useState("");
  const [rowGroup, setRowGroup] = useState<GroupMode>("none");
  const [rowHighlight, setRowHighlight] = useState<HighlightMode>("none");
  const [matrixGroup, setMatrixGroup] = useState(false);
  // Where the span of "Không vừa khoảng" ends; view state, not saved.
  const [spanEnd, setSpanEnd] = useState<SpanEnd>({ kind: "date", date: "" });
  // null: closed; 0: create a new series; otherwise the summary UID being edited.
  const [seriesDialog, setSeriesDialog] = useState<number | null>(null);
  // UID of the task the split dialog is open for.
  const [splitDialog, setSplitDialog] = useState<number | null>(null);
  const [evmOpen, setEvmOpen] = useState(false);
  const [aiOpen, setAiOpen] = useState(false);
  const [wbsOpen, setWbsOpen] = useState(false);
  // Bumped when the AI settings dialog closes, so the Recommendations panel reads them again.
  const [aiSettingsKey, setAiSettingsKey] = useState(0);
  const [metrics, setMetrics] = useState<ProjectMetrics | null>(null);
  const [networkSummaries, setNetworkSummaries] = useState(false);
  const [interruptsColumn, setInterruptsColumn] = useState(false);
  const [estimateColumns, setEstimateColumns] = useState(false);
  const [forecastColumns, setForecastColumns] = useState(false);
  // Custom fields left out of the sheet; a field added later shows until hidden.
  const [hiddenFields, setHiddenFields] = useState<ReadonlySet<number>>(() => new Set());
  const [sheetShare, setSheetShare] = useState(readShare);
  const changeSheetShare = useCallback((share: number) => setSheetShare(share), []);
  // Kept once a drag settles, not on every pointer move.
  useEffect(() => {
    const timer = window.setTimeout(() => saveShare(sheetShare), 300);
    return () => window.clearTimeout(timer);
  }, [sheetShare]);
  const [networkLayout, setNetworkLayout] = useState<NetworkLayout | null>(null);
  const [editUid, setEditUid] = useState<number | null>(null);
  const sheetRef = useRef<HTMLDivElement>(null);
  const ganttRef = useRef<HTMLDivElement>(null);
  const exportPdfRef = useRef<() => void>(() => {});
  const syncing = useRef(false);
  const lastAction = useRef(0);

  // `null` from `work` means nothing happened (no queued file to open): the last error
  // stays on screen instead of being cleared by a call the user did not make.
  const run = useCallback(async (work: () => Promise<Snapshot | void | null>): Promise<Snapshot | void> => {
    if (!inTauri()) {
      setError(pick(appMessages).needsTauri);
      return;
    }
    try {
      const next = await work();
      if (next === null) return;
      if (next) setSnap(next);
      setError(null);
      return next;
    } catch (err) {
      setError(errorMessage(err));
    }
  }, []);

  const send = useCallback((command: Command) => run(() => applyCommand(command)), [run]);
  // Thông tin dự án shows what the engine refused of its own edits in its footer, not in
  // the banner behind the dialog (where it would outlive the dialog after Lưu). Errors
  // from anything else stay in the banner.
  const [dialogError, setDialogError] = useState<string | null>(null);
  const applyPlanInDialog = useCallback(
    (plan: PlanPreview) =>
      run(async () => {
        try {
          const state = await applyPlan(plan);
          setDialogError(null);
          return state;
        } catch (err) {
          setDialogError(errorMessage(err));
          return undefined;
        }
      }),
    [run],
  );

  const sendInDialog = useCallback(
    (command: Command) =>
      run(async () => {
        try {
          const state = await applyCommand(command);
          setDialogError(null);
          return state;
        } catch (err) {
          setDialogError(errorMessage(err));
          return undefined;
        }
      }),
    [run],
  );

  const { startFileDialog, openFileWhenFree, fileDialogBusy } = useFileDialog(run, setError);

  const openWorkingTimeRef = useRef<(id: number) => Promise<void>>(async () => {});
  const createCalendar = useCallback(
    async (name: string, assign: (id: number) => Command | null, openEditor: boolean, baseId: number | null) => {
      const before = snap?.project.calendars.map((calendar) => calendar.id) ?? [];
      const result = await send({ type: "create_calendar", name, base_id: baseId });
      if (!result) return null;
      const created = result.project.calendars.find((calendar) => !before.includes(calendar.id));
      if (!created) return null;
      const command = assign(created.id);
      if (command) await send(command);
      if (openEditor) void openWorkingTimeRef.current(created.id);
      return created.id;
    },
    [send, snap],
  );

  // Read the cursor through the queue: a cell committed by the click that opened the
  // dialog may still be in flight, and must count as before the dialog, not inside it.
  const openProjectInfo = useCallback(
    async (tab: ProjectTab = "general") => {
      const state = await run(loadState);
      // Statistics from an earlier opening would show as current until the new ones load.
      setMetrics(null);
      setInfoTab(tab);
      setDialogError(null);
      setInfoStart(state ? state.history_cursor : (snap?.history_cursor ?? 0));
    },
    [run, snap],
  );

  // Lịch làm việc is a transaction like Thông tin dự án, from the same kind of cursor.
  const openWorkingTime = useCallback(
    async (id: number) => {
      const state = await run(loadState);
      setWorkingTime({ id, start: state ? state.history_cursor : (snap?.history_cursor ?? 0) });
    },
    [run, snap],
  );
  openWorkingTimeRef.current = openWorkingTime;

  // Khả dụng of a resource is a transaction too.
  const openAvailability = useCallback(
    async (uid: number) => {
      const state = await run(loadState);
      setDialogError(null);
      setAvailability({ uid, start: state ? state.history_cursor : (snap?.history_cursor ?? 0) });
    },
    [run, snap],
  );

  // Giao việc is a transaction too.
  const openAssignmentRules = useCallback(async () => {
    const state = await run(loadState);
    setDialogError(null);
    setAssignmentStart(state ? state.history_cursor : (snap?.history_cursor ?? 0));
  }, [run, snap]);

  // Lịch thanh toán is a transaction too.
  const openPaymentSchedule = useCallback(async () => {
    const state = await run(loadState);
    setDialogError(null);
    setPaymentsStart(state ? state.history_cursor : (snap?.history_cursor ?? 0));
  }, [run, snap]);

  // Suggestions are a read; reading them waits for queued edits, as every read does.
  const openAssignByRules = useCallback(async () => {
    if (!inTauri()) {
      setError(pick(appMessages).needsTauri);
      return;
    }
    try {
      const found = await suggestAssignments();
      setDialogError(null);
      setSuggestions(found);
    } catch (err) {
      setError(errorMessage(err));
    }
  }, []);

  // The file dialog goes through startFileDialog, so it never opens over another one.
  const openImportTable = useCallback(
    () =>
      startFileDialog(async () => {
        const sheet = await pickTable();
        if (sheet) {
          setDialogError(null);
          setImportSheet(sheet);
        }
      }),
    [startFileDialog],
  );

  // Undo every command applied since `cursor`. Queued edits (a field committed on
  // close) land first, because loadState waits for them.
  const rollbackTo = useCallback(
    (cursor: number) =>
      run(() => undoTo(cursor, { current: loadState, undo })),
    [run],
  );

  const menuRef = useRef<(id: string) => void>(() => {});
  // Selecting one row ends a Shift+click range.
  const selectOne = useCallback((uid: number) => {
    setSelected(uid);
    setSelectionEnd(null);
  }, []);

  const project = snap?.project;
  // Each forecast costs about a second of CPU: run it only while something shows it.
  const forecastState = useForecast(
    project,
    view === "forecast" || (forecastColumns && (view === "gantt" || view === "tracking")),
  );
  const forecastByTask = useMemo(
    () => (forecastState.current && forecastState.result ? new Map(forecastState.result.tasks.map((item) => [item.uid, item])) : null),
    [forecastState.current, forecastState.result],
  );
  // The health check reschedules a copy once: ask only while the dialog or its filter shows it.
  const healthCriterion = healthCriterionOf(rowFilter);
  const health = useScheduleHealth(project, healthOpen || healthCriterion != null);
  const healthTasks = useMemo(() => {
    const criterion = healthCriterion != null ? health.report?.criteria[healthCriterion - 1] : undefined;
    return criterion ? new Set(criterion.tasks.map((item) => item.uid)) : null;
  }, [health.report, healthCriterion]);
  const availabilityResource = availability ? project?.resources.find((item) => item.uid === availability.uid) : undefined;
  // A resource that is gone (a file opened elsewhere) ends its Khả dụng transaction.
  useEffect(() => {
    if (availability && project && !availabilityResource) setAvailability(null);
  }, [availability, project, availabilityResource]);
  const task = project?.tasks.find((item) => item.uid === selected) ?? null;
  const editTask = project?.tasks.find((item) => item.uid === editUid) ?? null;
  const rows = useMemo(
    () =>
      project
        ? buildViewRows(project, rowFilter, filterResource, rowGroup, rowHighlight, filterValue, spanEndInstant(project, spanEnd), healthTasks)
        : [],
    // Group headers ("Chưa gán" / "Unassigned") are text, so they follow the language.
    [project, rowFilter, filterResource, rowGroup, rowHighlight, filterValue, spanEnd, healthTasks, lang],
  );
  // A Shift+click range runs in the rows' order, also when picked on the network diagram.
  const selection = useMemo(
    () => {
      const range = selectionRange(rows, selected, selectionEnd);
      // A deleted or undone task leaves the selection before the effect below replaces it.
      return range.filter((uid) => project?.tasks.some((item) => item.uid === uid));
    },
    [rows, selected, selectionEnd, view, project],
  );
  const selectionSet = useMemo(() => new Set(selection), [selection]);
  const closeEdit = useCallback(() => setEditUid(null), []);

  // The theme switches at once; if the backend cannot store it, the previous one comes
  // back, so the screen, the menu check and settings.json agree.
  const changeTheme = useCallback((theme: ThemeChoice) => {
    const previous = themeChoice();
    applyTheme(theme);
    if (!inTauri()) return;
    setTheme(theme).catch((err) => {
      applyTheme(previous);
      setError(errorMessage(err));
    });
  }, []);

  const handleMenu = useCallback(
    (id: string) =>
      handleMenuAction(id, {
        snap,
        task,
        view,
        fileDialogBusy,
        // Undo stops where the open transaction began, in any of the dialogs.
        infoStart: infoStart ?? workingTime?.start ?? availability?.start ?? assignmentStart ?? paymentsStart ?? null,
        run,
        send,
        startFileDialog,
        exportPdf: () => exportPdfRef.current(),
        setView,
        setScale,
        zoomTimeline,
        setEvmOpen,
        setAiOpen,
        openProjectInfo: () => void openProjectInfo(),
        openCustomFields: () => void openProjectInfo("fields"),
        openLevelingOrder: () => void openProjectInfo("leveling"),
        openAssignmentRules: () => void openAssignmentRules(),
        openAssignByRules: () => void openAssignByRules(),
        openImportTable,
        openTracker: () => {
          setDialogError(null);
          setTrackerOpen(true);
        },
        openTrackerPush: () => {
          setDialogError(null);
          setTrackerPushOpen(true);
        },
        openTrackerSettings: () => {
          setDialogError(null);
          setTrackerSettingsOpen(true);
        },
        openWorkingTime: () => void openWorkingTime(snap?.project.calendar_id ?? 1),
        openPaymentSchedule: () => void openPaymentSchedule(),
        openBaseline: () => setBaselineOpen(true),
        openLicense: () => setLicenseOpen(true),
        openAiSettings: () => setAiSettingsOpen(true),
        openHealth: () => setHealthOpen(true),
        openRecurring: () => setSeriesDialog(0),
        openSplit: setSplitDialog,
        // A new, untouched project is renamed in the new language by the backend.
        changeLang: (next) =>
          void setLang(next)
            .then(() => run(loadState))
            .catch((err) => setError(errorMessage(err))),
        changeTheme,
        selection,
        clipboard,
        setClipboard,
        fillField,
        reportingPro,
        healthPro,
        cashflowPro,
        cashFlowOptions,
      }),
    [run, send, startFileDialog, task, snap, view, fileDialogBusy, openProjectInfo, openWorkingTime, openAssignmentRules, openAssignByRules, openImportTable, openPaymentSchedule, infoStart, workingTime, availability, assignmentStart, paymentsStart, setLang, changeTheme, selection, clipboard, fillField, setScale, zoomTimeline, reportingPro, healthPro, cashflowPro, cashFlowOptions],
  );
  menuRef.current = handleMenu;

  // Shortcuts and menu accelerators can both fire for one key press; act once, either on
  // the project or on the focused text field.
  const dispatchAction = useCallback(
    (id: string, focused: EventTarget | null) => {
      const now = Date.now();
      if (now - lastAction.current < 40) return;
      lastAction.current = now;
      if (shouldRunAction(id, focused)) menuRef.current(id);
      else if (isEditableTarget(focused)) runFieldAction(id, focused);
    },
    [],
  );

  useEffect(() => trackCommittedText(document), []);

  // A field the filter, the grouping or fill-down uses can be deleted (or undone away),
  // and the value a filter keeps can go: fall back rather than act on what is gone.
  useEffect(() => {
    if (!project) return;
    if (rowFilter.startsWith("field:") && !fieldOfMode(project, rowFilter)) setRowFilter("none");
    if (rowGroup.startsWith("field:") && !fieldOfMode(project, rowGroup)) setRowGroup("none");
    const filterField = fieldOfMode(project, rowFilter);
    if (filterField && filterValue !== "") {
      const keys = distinctValues(filterField, project.tasks).map(valueKey);
      if (!keys.includes(filterValue)) setFilterValue(keys[0] ?? "");
    }
    if (typeof fillField === "object" && fillField && !project.fields.some((field) => field.id === fillField.field)) {
      setFillField(null);
    }
    // A span ending at a milestone that is gone ends at the last finish instead.
    if (spanEnd.kind === "milestone" && spanEndInstant(project, spanEnd) == null) {
      setSpanEnd({ kind: "date", date: lastFinish(project) });
    }
  }, [project, rowFilter, rowGroup, filterValue, fillField, spanEnd]);

  const dispatchFocused = useCallback((id: string) => dispatchAction(id, document.activeElement), [dispatchAction]);
  const pasteTable = useCallback(
    (text: string) => {
      if (!project || fileDialogBusy || importSheet) return false;
      const sheet = parsePastedTable(text.normalize("NFC"));
      if (!sheet) return false;
      setDialogError(null);
      setImportSheet(sheet);
      return true;
    },
    [project, fileDialogBusy, importSheet],
  );
  useClipboardEvents(
    dispatchFocused,
    () => (project ? tasksAsText(project.tasks, selection, visibleFields(project.fields, hiddenFields)) : ""),
    pasteTable,
  );

  const title = windowTitle(snap);
  useEffect(() => {
    document.title = title;
    if (!inTauri()) return;
    getCurrentWindow()
      .setTitle(title)
      .catch((err) => setError(errorMessage(err)));
  }, [title]);

  useEffect(() => {
    if (!inTauri()) {
      setError(pick(appMessages).needsTauri);
      return;
    }
    // After a crash the backend offers the autosaved project back in a native dialog;
    // then a project the app was launched to open (a double-clicked file) opens.
    void run(() => loadState()).then(() =>
      startFileDialog(async () => {
        // A slot that cannot be restored must not keep the double-clicked file closed.
        const restored = await checkAutosave().catch((err: unknown) => {
          setError(errorMessage(err));
          return null;
        });
        return (await openPending()) ?? restored;
      }),
    );
    const stops: (() => void)[] = [];
    let cancelled = false;
    const keep = (stop: () => void) => {
      if (cancelled) stop();
      else stops.push(stop);
    };
    void onMenu((id) => dispatchAction(id, document.activeElement)).then(keep);
    void onOpenFile(openFileWhenFree).then(keep);
    return () => {
      cancelled = true;
      stops.forEach((stop) => stop());
    };
  }, [run, dispatchAction, startFileDialog, openFileWhenFree]);

  // A range whose far end was deleted does not come back when an undo restores it.
  useEffect(() => {
    if (selectionEnd != null && !project?.tasks.some((item) => item.uid === selectionEnd)) setSelectionEnd(null);
  }, [project, selectionEnd]);

  useEffect(() => {
    if (!project || project.tasks.length === 0) return;
    if (selected != null && project.tasks.some((item) => item.uid === selected)) return;
    setSelected(project.tasks[0].uid);
    setSelectionEnd(null);
  }, [project, selected]);

  // EVM, burn-down and project statistics are computed by the engine; refresh them after
  // every change while a panel or Thông tin dự án shows them.
  const infoOpen = infoStart != null;
  useEffect(() => {
    if ((!evmOpen && !infoOpen) || !snap || !inTauri()) return;
    let cancelled = false;
    loadMetrics()
      .then((next) => {
        if (!cancelled) setMetrics(next);
      })
      .catch((err) => {
        if (!cancelled) setError(errorMessage(err));
      });
    return () => {
      cancelled = true;
    };
  }, [evmOpen, infoOpen, snap]);

  useEffect(() => {
    if (!project || filterResource == null) return;
    if (project.resources.some((resource) => resource.uid === filterResource)) return;
    setFilterResource(project.resources[0]?.uid ?? null);
  }, [project, filterResource]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const id = shortcutId(event);
      if (!id) return;
      event.preventDefault();
      dispatchAction(id, event.target);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [dispatchAction]);

  // The network diagram ignores grouping; filters choose the boxes and highlight tints them.
  const networkRows = useMemo(
    () =>
      project && view === "network"
        ? buildViewRows(project, rowFilter, filterResource, "none", rowHighlight, filterValue, spanEndInstant(project, spanEnd), healthTasks)
        : [],
    [project, view, rowFilter, filterResource, rowHighlight, filterValue, spanEnd, healthTasks, lang],
  );
  const networkVisible = useMemo(
    () => (rowFilter === "none" ? null : networkRows.flatMap((row) => (row.kind === "task" ? [row.task.uid] : []))),
    [networkRows, rowFilter],
  );
  const networkHighlighted = useMemo(
    () => new Set(networkRows.flatMap((row) => (row.kind === "task" && row.highlighted ? [row.task.uid] : []))),
    [networkRows],
  );

  const matrixRows = useMemo(
    () =>
      project && view === "matrix"
        ? buildViewRows(project, rowFilter, filterResource, "none", "none", filterValue, spanEndInstant(project, spanEnd), healthTasks)
        : [],
    [project, view, rowFilter, filterResource, filterValue, spanEnd, healthTasks, lang],
  );
  const matrixVisible = useMemo(
    () => (rowFilter === "none" ? null : new Set(matrixRows.flatMap((row) => (row.kind === "task" ? [row.task.uid] : [])))),
    [matrixRows, rowFilter],
  );

  // Layout comes from the engine, which also owns the schedule; refresh it after every change.
  useEffect(() => {
    if (view !== "network" || !snap || !inTauri()) return;
    let cancelled = false;
    loadNetworkLayout({ include_summary: networkSummaries, visible_uids: networkVisible })
      .then((next) => {
        if (!cancelled) setNetworkLayout(next);
      })
      .catch((err) => {
        if (!cancelled) setError(errorMessage(err));
      });
    return () => {
      cancelled = true;
    };
  }, [view, snap, networkSummaries, networkVisible]);

  exportPdfRef.current = () => {
    if (!project) return;
    if (view === "network") {
      const layout = networkLayout;
      if (!layout) return;
      startFileDialog(async () => {
        const bytes = buildNetworkPdf({ project, layout, highlighted: networkHighlighted });
        await savePdf(bytes, pdfSuggestedName(m.networkPdfName(project.name.trim() || m.untitledProject)));
      });
      return;
    }
    startFileDialog(async () => {
      const bytes = buildGanttPdf({
        project,
        rows,
        scale,
        selected,
        tracking: view === "tracking",
        fields: visibleFields(project.fields, hiddenFields),
      });
      await savePdf(bytes, pdfSuggestedName(project.name));
    });
  };
  const splitTask = project?.tasks.find((item) => item.uid === splitDialog) ?? null;

  function syncScroll(source: "sheet" | "gantt") {
    if (syncing.current) return;
    const from = source === "sheet" ? sheetRef.current : ganttRef.current;
    const to = source === "sheet" ? ganttRef.current : sheetRef.current;
    if (!from || !to) return;
    syncing.current = true;
    to.scrollTop = from.scrollTop;
    syncing.current = false;
  }

  return (
    <div className="app">
      <Toolbar
        states={toolbarStates({ snap, task, view, fileDialogBusy, selection, hasClipboard: clipboard != null, fillField })}
        evmOpen={evmOpen}
        aiOpen={aiOpen}
        onUndo={() => void run(undo)}
        onRedo={() => void run(redo)}
        onAddTask={() =>
          void send({ type: "add_task", name: m.newTask(project?.next_task_uid ?? 1), after_uid: selected })
        }
        onAddMilestone={() => void send({ type: "add_milestone", name: m.newMilestone, after_uid: selected })}
        onAddRecurring={() => setSeriesDialog(0)}
        onIndent={() => task && void send({ type: "indent", uid: task.uid })}
        onOutdent={() => task && void send({ type: "outdent", uid: task.uid })}
        onLink={(kind) => linkAbove(project, task, kind, send)}
        onUnlink={() => {
          const link = linkFromAbove(snap, task);
          if (link) void send({ type: "remove_dependency", uid: link.uid });
        }}
        onSplit={() => task && setSplitDialog(task.uid)}
        onDelete={() => task && void send({ type: "delete_task", uid: task.uid })}
        onProjectInfo={() => void openProjectInfo()}
        onAssignByRules={() => void openAssignByRules()}
        onToggleEvm={() => {
          if (!evmOpen && view !== "gantt" && view !== "tracking") setView("gantt");
          setEvmOpen((open) => !open);
        }}
        onToggleAi={() => {
          if (!aiOpen && view !== "gantt" && view !== "tracking") setView("gantt");
          setAiOpen((open) => !open);
        }}
        onExportPdf={() => exportPdfRef.current()}
      />
      <ViewBar
        project={project}
        view={view}
        onView={setView}
        scale={scale}
        onScale={setScale}
        zoom={zoom}
        onZoomReset={() => {
          setZoom(1);
          // The button goes away with the zoom; keep keyboard focus on the chart.
          ganttRef.current?.focus();
        }}
        filter={rowFilter}
        onFilter={setRowFilter}
        filterResource={filterResource}
        onFilterResource={setFilterResource}
        filterValue={filterValue}
        onFilterValue={setFilterValue}
        group={rowGroup}
        onGroup={setRowGroup}
        highlight={rowHighlight}
        onHighlight={setRowHighlight}
        matrixGroup={matrixGroup}
        onMatrixGroup={setMatrixGroup}
        networkSummaries={networkSummaries}
        onNetworkSummaries={setNetworkSummaries}
        interruptsColumn={interruptsColumn}
        onInterruptsColumn={setInterruptsColumn}
        estimateColumns={estimateColumns}
        onEstimateColumns={setEstimateColumns}
        forecastColumns={forecastColumns}
        onForecastColumns={setForecastColumns}
        hiddenFields={hiddenFields}
        onFieldColumn={(fieldId, show) => setHiddenFields((hidden) => toggleHidden(hidden, fieldId, show))}
        spanEnd={spanEnd}
        onSpanEnd={setSpanEnd}
      />
      {error && <div className="banner">{error}</div>}
      {/* EVM sits beside the Gantt and the inspector so its burn-down chart keeps the full height. */}
      <div className="work-area">
        <div className="work-column">
          <main className="workspace">
            {/* The panel the view tabs control; inside <main> so the main landmark stays. */}
            <div className="workspace-panel" id={VIEW_PANEL_ID} role="tabpanel" aria-labelledby={viewTabId(view)}>
            {project && (view === "gantt" || view === "tracking") && (
              <>
                {activeDiffReport && (
                  <div className="diff-gantt-banner">
                    <GitCompare size={15} />
                    <span>
                      {m.diffBannerTitle(
                        activeDiffReport.base_name,
                        activeDiffReport.summary.modified_count +
                          activeDiffReport.summary.added_count +
                          activeDiffReport.summary.removed_count,
                      )}
                    </span>
                    <button
                      type="button"
                      className="diff-banner-action"
                      onClick={() => setView("diff")}
                    >
                      {m.diffBannerDetails}
                    </button>
                    <button
                      type="button"
                      className="diff-banner-close"
                      onClick={() => setActiveDiffReport(null)}
                      title={m.diffBannerClose}
                    >
                      <X size={14} />
                    </button>
                  </div>
                )}
                <TaskSheet
                  rows={rows}
                  calendars={project.calendars}
                  fields={visibleFields(project.fields, hiddenFields)}
                  assignments={project.assignments}
                  resources={project.resources}
                  selected={selected}
                  selection={selectionSet}
                  tracking={view === "tracking"}
                  interruptsColumn={interruptsColumn}
                  estimateColumns={estimateColumns}
                  forecastColumns={forecastColumns}
                  forecast={forecastByTask}
                  onWorkRange={(uid, low, high) => send({ type: "set_work_range", uid, low, high })}
                  share={sheetShare}
                  bodyRef={sheetRef}
                  onSelect={(uid, extend) => (extend && selected != null ? setSelectionEnd(uid) : selectOne(uid))}
                  onFillField={setFillField}
                  onRename={(uid, name) => send({ type: "rename_task", uid, name })}
                  onDuration={(uid, minutes) => send({ type: "set_duration", uid, minutes })}
                  onStart={(uid, start) => send({ type: "move_task", uid, start })}
                  onPredecessors={(uid, text) => send({ type: "set_predecessors", uid, text: normalizePredecessorText(text) })}
                  onPercent={(uid, percent) => send({ type: "set_percent_complete", uid, percent })}
                  onRemaining={(uid, minutes) => send({ type: "set_remaining_duration", uid, minutes })}
                  onActualStart={(uid, start) => send({ type: "set_actual_start", uid, start })}
                  onActualFinish={(uid, finish) => send({ type: "set_actual_finish", uid, finish })}
                  onRemainingWork={(uid, minutes) => send({ type: "set_remaining_work", uid, minutes })}
                  onOvertime={(uid, minutes) => send({ type: "set_overtime_work", uid, minutes })}
                  onCalendar={(uid, calendarId) => send({ type: "set_task_calendar", uid, calendar_id: calendarId })}
                  onFieldValue={(uid, fieldId, value) => send({ type: "set_field_value", uid, field_id: fieldId, value })}
                  onInterrupts={(uid, interrupts) => send({ type: "set_interrupts", uid, interrupts })}
                  onEditSeries={(uid) => {
                    selectOne(uid);
                    setSeriesDialog(uid);
                  }}
                  onScroll={() => syncScroll("sheet")}
                />
                <PaneSplitter share={sheetShare} onChange={changeSheetShare} />
                <GanttChart
                  project={project}
                  rows={rows}
                  scale={scale}
                  zoom={zoom}
                  onZoom={setZoom}
                  selected={selected}
                  tracking={view === "tracking"}
                  bodyRef={ganttRef}
                  onSelect={selectOne}
                  selectionEnd={selectionEnd}
                  onExtend={(uid) => setSelectionEnd(uid === selected ? null : uid)}
                  onMove={(uid, start) => send({ type: "move_task", uid, start })}
                  onResize={(uid, finish) => send({ type: "resize_task", uid, finish })}
                  onMoveSegment={(uid, index, start) => send({ type: "move_segment", uid, index, start })}
                  onLink={(from, to) => send({ type: "add_dependency", predecessor_uid: from, successor_uid: to, kind: "fs", lag: 0 })}
                  onEdit={setEditUid}
                  onScroll={() => syncScroll("gantt")}
                  diffReport={activeDiffReport}
                />
              </>
            )}
            {project && view === "resources" && (
              <ResourceSheet
                resources={project.resources}
                calendars={project.calendars}
                skills={project.skills}
                skillLevels={project.skill_levels}
                focusFactor={project.focus_factor}
                capacityPro={capacityPro}
                overallocations={project.overallocations}
                onAdd={(kind) => send({ type: "add_resource", name: m.newResource(project.next_resource_uid), kind })}
                onChange={(resource) => send(resourceCommand(resource))}
                onChangeEmail={(uid, email) => send({ type: "set_resource_email", uid, email })}
                onChangeGroup={(uid, group) => send({ type: "set_resource_group", uid, group })}
                onChangeFocus={(uid, focus) => send({ type: "set_resource_focus", uid, focus })}
                onDelete={(uid) => send({ type: "delete_resource", uid })}
                onEditCalendar={(id) => void openWorkingTime(id)}
                onEditAvailability={(uid) => void openAvailability(uid)}
                onCreateCalendar={(resource) => {
                  const name = resource.name.trim() ? m.calendarFor(resource.name.trim()) : m.resourceCalendar;
                  // Based on the project calendar, so its holidays reach the resource.
                  void createCalendar(name, (id) => resourceCommand({ ...resource, calendar_id: id }), true, project.calendar_id);
                }}
              />
            )}
            {project && view === "matrix" && (
              <AssignmentMatrixView
                project={project}
                group={matrixGroup}
                visibleTaskUids={matrixVisible}
                onAssign={(taskUid, resourceUid, units) =>
                  void send({ type: "assign_resource", task_uid: taskUid, resource_uid: resourceUid, units })
                }
                onUpdate={(uid, units) =>
                  void send({ type: "update_assignment", uid, units, work: null, overtime_work: null })
                }
                onRemove={(uid) => void send({ type: "remove_assignment", uid })}
              />
            )}
            {project && view === "usage" && <TaskUsage project={project} />}
            {project && view === "load" && (
              <LoadMapView
                project={project}
                onPickResource={(uid) => {
                  setRowFilter("resource");
                  setFilterResource(uid);
                  setView("gantt");
                }}
              />
            )}
            {project && view === "curves" && <CurvesView project={project} />}
            {project && view === "forecast" && (
              <ForecastView
                project={project}
                state={forecastState}
                onPickTask={(uid) => {
                  selectOne(uid);
                  setView("gantt");
                }}
              />
            )}
            {project && view === "cashflow" && (
              <CashFlowView
                project={project}
                cashflowPro={cashflowPro}
                onOpenLicense={() => setLicenseOpen(true)}
                onOpenPayments={() => void openPaymentSchedule()}
                onOptionsChange={setCashFlowOptions}
                onExport={() => dispatchFocused("export-cash-flow")}
                exportBusy={fileDialogBusy}
                onPickTask={(uid) => {
                  selectOne(uid);
                  setView("gantt");
                }}
              />
            )}
            {project && view === "diff" && (
              <VarianceAuditView
                diffPro={diffPro}
                onOpenLicense={() => setLicenseOpen(true)}
                activeReport={activeDiffReport}
                onReportChange={setActiveDiffReport}
                onPickTask={(uid) => {
                  selectOne(uid);
                  setView("gantt");
                }}
              />
            )}
            {project && view === "network" && (
              <NetworkDiagram
                project={project}
                layout={networkLayout}
                selected={selected}
                selection={selectionSet}
                highlighted={networkHighlighted}
                onSelect={(uid, extend) => (extend && selected != null ? setSelectionEnd(uid) : selectOne(uid))}
                onEdit={(uid) => {
                  selectOne(uid);
                  setEditUid(uid);
                }}
              />
            )}
            </div>
          </main>
          {project && task && (view === "gantt" || view === "tracking") && (
            <Inspector
              project={project}
              task={task}
              send={send}
              onEditSeries={setSeriesDialog}
              onSplit={setSplitDialog}
              onEditCalendar={(id) => void openWorkingTime(id)}
              onOpenPayments={() => void openPaymentSchedule()}
              onCreateCalendar={() => {
                const name = task.name.trim() ? m.calendarFor(task.name.trim()) : m.taskCalendar;
                void createCalendar(
                  name,
                  (id) => ({ type: "set_task_calendar", uid: task.uid, calendar_id: id }),
                  true,
                  project.calendar_id,
                );
              }}
            />
          )}
        </div>
        {project && evmOpen && (view === "gantt" || view === "tracking") && (
          <EvmPanel
            metrics={metrics}
            onOpenProjectInfo={() => void openProjectInfo()}
            onOpenBaseline={() => setBaselineOpen(true)}
            onClose={() => setEvmOpen(false)}
          />
        )}
        {project && aiOpen && (view === "gantt" || view === "tracking") && (
          <RecommendationsPanel
            project={project}
            aiPro={license?.features.includes("ai") ?? false}
            settingsKey={aiSettingsKey}
            onOpenLicense={() => setLicenseOpen(true)}
            onOpenSettings={() => setAiSettingsOpen(true)}
            onPickTask={selectOne}
            onApplied={(next) => void run(async () => next)}
            onOpenWbs={() => setWbsOpen(true)}
            onClose={() => setAiOpen(false)}
          />
        )}
      </div>
      <StatusBar snap={snap} />
      {infoStart != null && project && (
        <ProjectDialog
          project={project}
          statistics={metrics?.statistics ?? null}
          changed={(snap?.history_cursor ?? 0) !== infoStart}
          initialTab={infoTab}
          error={dialogError}
          capacityPro={capacityPro}
          onSave={() => {
            commitFocusedField();
            setInfoStart(null);
          }}
          onCancel={() => {
            void rollbackTo(infoStart).then((state) => {
              // Close only once everything is undone; otherwise stay open so Hủy can be
              // tried again (or Lưu kept) instead of leaving the project half reverted.
              if (state && state.history_cursor <= infoStart) setInfoStart(null);
              else setError(m.rollbackFailed);
            });
          }}
          send={sendInDialog}
          onOpenWorkingTime={() => {
            // Keep what was changed here, then edit working time in its own transaction.
            commitFocusedField();
            setInfoStart(null);
            void openWorkingTime(project.calendar_id);
          }}
        />
      )}
      {workingTime && project && (
        <WorkingTimeDialog
          project={project}
          initialId={workingTime.id}
          changed={(snap?.history_cursor ?? 0) !== workingTime.start}
          onSave={() => {
            commitFocusedField();
            setWorkingTime(null);
          }}
          onCancel={() => {
            const start = workingTime.start;
            void rollbackTo(start).then((state) => {
              if (state && state.history_cursor <= start) setWorkingTime(null);
              else setError(m.rollbackFailed);
            });
          }}
          send={send}
          onCreateCalendar={() => createCalendar(m.newCalendar, () => null, false, null)}
        />
      )}
      {availability && project && availabilityResource && (
        <ResourceAvailabilityDialog
          resource={availabilityResource}
          changed={(snap?.history_cursor ?? 0) !== availability.start}
          error={dialogError}
          today={instantToDateKey(todayInstant())}
          onSave={() => {
            commitFocusedField();
            setAvailability(null);
          }}
          onCancel={() => {
            const start = availability.start;
            void rollbackTo(start).then((state) => {
              if (state && state.history_cursor <= start) setAvailability(null);
              else setError(m.rollbackFailed);
            });
          }}
          send={sendInDialog}
        />
      )}
      {assignmentStart != null && project && (
        <AssignmentDialog
          project={project}
          changed={(snap?.history_cursor ?? 0) !== assignmentStart}
          error={dialogError}
          onSave={() => setAssignmentStart(null)}
          onCancel={() => {
            const start = assignmentStart;
            void rollbackTo(start).then((state) => {
              if (state && state.history_cursor <= start) setAssignmentStart(null);
              else setError(m.rollbackFailed);
            });
          }}
          send={sendInDialog}
        />
      )}
      {paymentsStart != null && project && (
        <PaymentScheduleDialog
          project={project}
          changed={(snap?.history_cursor ?? 0) !== paymentsStart}
          error={dialogError}
          today={utcMidnight(todayInstant())}
          onSave={() => setPaymentsStart(null)}
          onCancel={() => {
            const start = paymentsStart;
            void rollbackTo(start).then((state) => {
              if (state && state.history_cursor <= start) setPaymentsStart(null);
              else setError(m.rollbackFailed);
            });
          }}
          send={sendInDialog}
        />
      )}
      {suggestions && project && (
        <AssignPreviewDialog
          project={project}
          suggestions={suggestions}
          error={dialogError}
          applyPlan={applyPlanInDialog}
          clearError={() => setDialogError(null)}
          onClose={() => {
            setDialogError(null);
            setSuggestions(null);
          }}
        />
      )}
      {wbsOpen && project && (
        <WbsDialog
          project={project}
          afterTask={task}
          error={dialogError}
          send={sendInDialog}
          onClose={() => {
            setDialogError(null);
            setWbsOpen(false);
          }}
        />
      )}
      {importSheet && project && (
        <ImportDialog
          project={project}
          initial={importSheet}
          afterTask={task}
          error={dialogError}
          send={sendInDialog}
          onClose={() => {
            setDialogError(null);
            setImportSheet(null);
          }}
        />
      )}
      {trackerSettingsOpen && project && (
        <TrackerSettingsDialog
          project={project}
          error={dialogError}
          send={sendInDialog}
          onClose={() => {
            setDialogError(null);
            setTrackerSettingsOpen(false);
          }}
        />
      )}
      {trackerPushOpen && project && (
        <TrackerPushDialog
          project={project}
          error={dialogError}
          send={sendInDialog}
          onClose={() => {
            setDialogError(null);
            setTrackerPushOpen(false);
          }}
        />
      )}
      {trackerOpen && project && (
        <TrackerDialog
          project={project}
          error={dialogError}
          send={sendInDialog}
          onClose={() => {
            setDialogError(null);
            setTrackerOpen(false);
          }}
        />
      )}
      {seriesDialog != null && project && (
        <RecurringDialog
          key={seriesDialog}
          project={project}
          initial={project.tasks.find((item) => item.uid === seriesDialog)?.recurrence?.spec ?? null}
          defaultStart={task && !task.summary ? task.start : project.start}
          onClose={() => setSeriesDialog(null)}
          onSubmit={(spec) => send(recurringCommand(project.tasks, seriesDialog, selected, spec))}
        />
      )}
      {splitTask && (
        <SplitDialog
          key={splitTask.uid}
          task={splitTask}
          onClose={() => setSplitDialog(null)}
          onSubmit={(at, gap) => send({ type: "split_task", uid: splitTask.uid, at, gap })}
        />
      )}
      {editTask && <TaskEditDialog key={editTask.uid} task={editTask} send={send} onClose={closeEdit} />}
      {baselineOpen && (
        <BaselineDialog
          onClose={() => setBaselineOpen(false)}
          onSet={(number) => send({ type: "set_baseline", number })}
          onClear={(number) => send({ type: "clear_baseline", number })}
        />
      )}
      {aiSettingsOpen && (
        <AiSettingsDialog
          aiPro={license?.features.includes("ai") ?? false}
          onClose={() => {
            setAiSettingsOpen(false);
            setAiSettingsKey((key) => key + 1);
          }}
          onOpenLicense={() => {
            setAiSettingsOpen(false);
            setAiSettingsKey((key) => key + 1);
            setLicenseOpen(true);
          }}
        />
      )}
      {healthOpen && (
        <HealthDialog
          report={health.report}
          current={health.current}
          error={health.error}
          onClose={() => setHealthOpen(false)}
          onPick={(number) => {
            setHealthOpen(false);
            setRowFilter(`health:${number}`);
            if (view !== "gantt" && view !== "tracking") setView("gantt");
          }}
        />
      )}
      {licenseOpen && <LicenseDialog onClose={() => setLicenseOpen(false)} onChange={setLicense} />}
    </div>
  );
}
