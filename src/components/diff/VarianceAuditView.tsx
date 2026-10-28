import { ArrowRight, FileSpreadsheet, FolderOpen, GitCompare, KeyRound, Search, X } from "lucide-react";
import { useMemo, useState } from "react";

import { errorMessage, pickAndCompareProject, saveTextReport } from "../../api";
import { useMessages } from "../../i18n";
import type { ProjectDiffReport, TaskDiff } from "../../types";
import { Button } from "../Button";
import { ProBadge } from "../license/ProBadge";
import { varianceAuditMessages } from "./VarianceAuditView.i18n";

interface Props {
  diffPro: boolean;
  onOpenLicense: () => void;
  activeReport: ProjectDiffReport | null;
  onReportChange: (report: ProjectDiffReport | null) => void;
  onPickTask: (uid: number) => void;
}

type FilterMode = "all" | "changes" | "slipped";

export function VarianceAuditView({
  diffPro,
  onOpenLicense,
  activeReport,
  onReportChange,
  onPickTask,
}: Props) {
  const m = useMessages(varianceAuditMessages);
  const [filterMode, setFilterMode] = useState<FilterMode>("all");
  const [searchQuery, setSearchQuery] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handlePickFile = async () => {
    try {
      setLoading(true);
      setError(null);
      const report = await pickAndCompareProject();
      if (report) {
        onReportChange(report);
      }
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setLoading(false);
    }
  };

  const handleExportCsv = async () => {
    if (!activeReport) return;
    try {
      const headers = [
        m.colWbs,
        m.colName,
        m.colStatus,
        "Old Start",
        "New Start",
        "Old Finish",
        "New Finish",
        m.colSlip,
        "Old Work (h)",
        "New Work (h)",
        "Work Delta (h)",
        "Old Cost",
        "New Cost",
        "Cost Delta",
        m.colLinks,
        m.colAssign,
      ];
      const rows = activeReport.tasks.map((t) => [
        t.wbs,
        `"${(t.name || "").replace(/"/g, '""')}"`,
        t.kind,
        t.old_start ?? "",
        t.new_start ?? "",
        t.old_finish ?? "",
        t.new_finish ?? "",
        t.finish_slip_days.toFixed(1),
        t.old_work != null ? (t.old_work / 60).toFixed(1) : "",
        t.new_work != null ? (t.new_work / 60).toFixed(1) : "",
        (t.work_delta / 60).toFixed(1),
        t.old_cost != null ? t.old_cost.toFixed(2) : "",
        t.new_cost != null ? t.new_cost.toFixed(2) : "",
        t.cost_delta.toFixed(2),
        `"${t.link_diffs.map((l) => `${l.change}:${l.other_task_name}`).join("; ")}"`,
        `"${t.assignment_diffs.map((a) => `${a.change}:${a.resource_name}`).join("; ")}"`,
      ]);
      const csvContent = "\uFEFF" + [headers.join(","), ...rows.map((r) => r.join(","))].join("\n");
      await saveTextReport(csvContent, "project-variance-audit.csv");
    } catch (err) {
      setError(errorMessage(err));
    }
  };

  const filteredTasks = useMemo(() => {
    if (!activeReport) return [];
    return activeReport.tasks.filter((task) => {
      // Filter by mode
      if (filterMode === "changes" && task.kind === "Unchanged") return false;
      if (filterMode === "slipped" && task.finish_slip_days <= 0) return false;

      // Filter by text search
      if (searchQuery.trim()) {
        const query = searchQuery.toLowerCase();
        const matchesName = task.name.toLowerCase().includes(query);
        const matchesWbs = task.wbs.toLowerCase().includes(query);
        if (!matchesName && !matchesWbs) return false;
      }
      return true;
    });
  }, [activeReport, filterMode, searchQuery]);

  if (!diffPro) {
    return (
      <section className="diff-view" aria-label={m.title}>
        <div className="diff-intro">
          <strong>
            {m.introTitle}
            <ProBadge />
          </strong>
          <p>{m.intro}</p>
          <div className="diff-actions">
            <Button variant="primary" Icon={KeyRound} onClick={onOpenLicense}>
              {m.activate}
            </Button>
          </div>
        </div>
      </section>
    );
  }

  return (
    <section className="diff-view" aria-label={m.title}>
      {/* Top Toolbar */}
      <div className="diff-toolbar">
        <Button Icon={FolderOpen} variant={activeReport ? "secondary" : "primary"} onClick={handlePickFile} disabled={loading}>
          {m.pickFile}
        </Button>

        {activeReport && (
          <>
            <span className="diff-target-badge" title={activeReport.base_name}>
              <GitCompare size={15} />
              <span>{m.comparingWith(activeReport.base_name)}</span>
              <button
                type="button"
                className="diff-close-btn"
                title={m.closeDiff}
                onClick={() => onReportChange(null)}
              >
                <X size={14} />
              </button>
            </span>

            <span className="spacer" />

            <Button Icon={FileSpreadsheet} onClick={handleExportCsv} title={m.exportCsv}>
              {m.exportCsv}
            </Button>
          </>
        )}
      </div>

      {/* Loading & Errors */}
      {loading && <p className="diff-empty">{m.loading}</p>}
      {error && (
        <p className="diff-empty" role="alert">
          {m.failed(error)}
        </p>
      )}

      {/* When no file selected yet */}
      {!loading && !error && !activeReport && (
        <div className="diff-placeholder">
          <GitCompare size={48} className="diff-placeholder-icon" />
          <p>{m.intro}</p>
          <Button variant="primary" Icon={FolderOpen} onClick={handlePickFile}>
            {m.pickFile}
          </Button>
        </div>
      )}

      {/* Active Diff Content */}
      {!loading && activeReport && (
        <div className="diff-content">
          {/* Summary KPIs */}
          <div className="diff-summary-cards">
            <div className="diff-card added">
              <span className="diff-card-label">{m.summaryAdded(activeReport.summary.added_count)}</span>
            </div>
            <div className="diff-card removed">
              <span className="diff-card-label">{m.summaryRemoved(activeReport.summary.removed_count)}</span>
            </div>
            <div className="diff-card modified">
              <span className="diff-card-label">{m.summaryModified(activeReport.summary.modified_count)}</span>
            </div>
            <div className={`diff-card ${activeReport.summary.slipped_count > 0 ? "slipped" : "good"}`}>
              <span className="diff-card-label">{m.summarySlipped(activeReport.summary.slipped_count)}</span>
            </div>
            {activeReport.finish_delta_days !== 0 && (
              <div className={`diff-card ${activeReport.finish_delta_days > 0 ? "slipped" : "good"}`}>
                <span className="diff-card-label">
                  Δ Finish: {m.daysSlip(activeReport.finish_delta_days)}
                </span>
              </div>
            )}
          </div>

          {/* Filters & Search */}
          <div className="diff-filter-bar">
            <div className="segmented" role="group">
              <button
                type="button"
                className={filterMode === "all" ? "on" : ""}
                onClick={() => setFilterMode("all")}
              >
                {m.allTasks}
              </button>
              <button
                type="button"
                className={filterMode === "changes" ? "on" : ""}
                onClick={() => setFilterMode("changes")}
              >
                {m.onlyChanges}
              </button>
              <button
                type="button"
                className={filterMode === "slipped" ? "on" : ""}
                onClick={() => setFilterMode("slipped")}
              >
                {m.onlySlipped}
              </button>
            </div>

            <div className="diff-search">
              <Search size={14} />
              <input
                type="text"
                placeholder={m.searchPlaceholder}
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
              />
              {searchQuery && (
                <button type="button" onClick={() => setSearchQuery("")}>
                  <X size={12} />
                </button>
              )}
            </div>
          </div>

          {/* Diff Table */}
          <div className="diff-table-container">
            <table className="diff-table">
              <thead>
                <tr>
                  <th style={{ width: 60 }}>{m.colWbs}</th>
                  <th>{m.colName}</th>
                  <th style={{ width: 90 }}>{m.colStatus}</th>
                  <th style={{ width: 170 }}>{m.colDates}</th>
                  <th style={{ width: 100 }}>{m.colSlip}</th>
                  <th style={{ width: 100 }}>{m.colWork}</th>
                  <th style={{ width: 100 }}>{m.colCost}</th>
                  <th>{m.colLinks}</th>
                  <th>{m.colAssign}</th>
                </tr>
              </thead>
              <tbody>
                {filteredTasks.length === 0 ? (
                  <tr>
                    <td colSpan={9} className="diff-no-match">
                      {m.noTasksMatch}
                    </td>
                  </tr>
                ) : (
                  filteredTasks.map((task) => (
                    <TaskDiffRow
                      key={task.uid}
                      task={task}
                      m={m}
                      onPickTask={onPickTask}
                    />
                  ))
                )}
              </tbody>
            </table>
          </div>
        </div>
      )}
    </section>
  );
}

function TaskDiffRow({
  task,
  m,
  onPickTask,
}: {
  task: TaskDiff;
  m: any;
  onPickTask: (uid: number) => void;
}) {
  const statusLabel =
    task.kind === "Added"
      ? m.statusAdded
      : task.kind === "Removed"
      ? m.statusRemoved
      : task.kind === "Modified"
      ? m.statusModified
      : m.statusUnchanged;

  const isSlipped = task.finish_slip_days > 0;
  const isEarly = task.finish_slip_days < 0;
  const workDeltaHours = task.work_delta / 60;

  return (
    <tr className={`diff-row kind-${task.kind.toLowerCase()} ${isSlipped ? "is-slipped" : ""}`}>
      <td className="diff-cell-wbs">{task.wbs}</td>
      <td className="diff-cell-name">
        <button
          type="button"
          className="diff-task-link"
          onClick={() => onPickTask(task.uid)}
          title={`UID ${task.uid}`}
        >
          {task.name || <em>(Untitled)</em>}
        </button>
      </td>
      <td className="diff-cell-status">
        <span className={`diff-badge badge-${task.kind.toLowerCase()}`}>{statusLabel}</span>
      </td>
      <td className="diff-cell-dates">
        {task.kind === "Added" ? (
          <span>{task.new_finish || task.new_start}</span>
        ) : task.kind === "Removed" ? (
          <span className="diff-old-val">{task.old_finish || task.old_start}</span>
        ) : task.old_finish === task.new_finish ? (
          <span>{task.new_finish}</span>
        ) : (
          <div className="diff-date-change">
            <span className="diff-old-val">{task.old_finish}</span>
            <ArrowRight size={12} className="diff-arrow" />
            <span className="diff-new-val">{task.new_finish}</span>
          </div>
        )}
      </td>
      <td className={`diff-cell-slip ${isSlipped ? "slip-late" : isEarly ? "slip-early" : ""}`}>
        {task.finish_slip_days === 0
          ? m.onTime
          : m.daysSlip(task.finish_slip_days)}
      </td>
      <td className="diff-cell-work">
        {workDeltaHours !== 0 ? (
          <span className={workDeltaHours > 0 ? "delta-up" : "delta-down"}>
            {workDeltaHours > 0 ? `+${workDeltaHours.toFixed(1)}h` : `${workDeltaHours.toFixed(1)}h`}
          </span>
        ) : (
          "—"
        )}
      </td>
      <td className="diff-cell-cost">
        {task.cost_delta !== 0 ? (
          <span className={task.cost_delta > 0 ? "delta-up" : "delta-down"}>
            {task.cost_delta > 0 ? `+$${task.cost_delta.toFixed(0)}` : `$${task.cost_delta.toFixed(0)}`}
          </span>
        ) : (
          "—"
        )}
      </td>
      <td className="diff-cell-links">
        {task.link_diffs.length === 0 ? (
          "—"
        ) : (
          <div className="diff-chips">
            {task.link_diffs.map((link, idx) => (
              <span key={idx} className={`diff-chip chip-${link.change}`}>
                {link.change === "added" ? "+" : link.change === "removed" ? "-" : "~"}
                {link.other_task_name}
              </span>
            ))}
          </div>
        )}
      </td>
      <td className="diff-cell-assign">
        {task.assignment_diffs.length === 0 ? (
          "—"
        ) : (
          <div className="diff-chips">
            {task.assignment_diffs.map((asgn, idx) => (
              <span key={idx} className={`diff-chip chip-${asgn.change}`}>
                {asgn.change === "added" ? "+" : asgn.change === "removed" ? "-" : "~"}
                {asgn.resource_name}
              </span>
            ))}
          </div>
        )}
      </td>
    </tr>
  );
}
