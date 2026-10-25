import { useEffect, useRef, useState } from "react";

import { errorMessage, scheduleHealth } from "../../api";
import type { HealthReport, Project } from "../../types";

export interface HealthState {
  /** The last report, possibly for an older version of the project. */
  report: HealthReport | null;
  /** Whether `report` is for the project as it is now. */
  current: boolean;
  error: string | null;
}

/**
 * The schedule health check of `project`, asked again whenever the project changes while
 * `wanted` (the dialog is open or the sheet is filtered by a criterion). A check reschedules
 * a copy once (criterion 12), tens of milliseconds at 5,000 tasks, so nothing runs otherwise;
 * an answer to an older request is dropped.
 */
export function useScheduleHealth(project: Project | undefined, wanted: boolean): HealthState {
  const [state, setState] = useState<{ report: HealthReport | null; forProject: Project | null; error: string | null }>({
    report: null,
    forProject: null,
    error: null,
  });
  const latest = useRef(0);
  const shown = useRef<Project | null>(null);
  shown.current = state.forProject;

  useEffect(() => {
    if (!wanted || !project || shown.current === project) return;
    const request = ++latest.current;
    scheduleHealth().then(
      (report) => {
        if (request === latest.current) setState({ report, forProject: project, error: null });
      },
      (err) => {
        if (request === latest.current) setState((before) => ({ ...before, error: errorMessage(err) }));
      },
    );
  }, [project, wanted]);

  return { report: state.report, current: state.report != null && state.forProject === project, error: state.error };
}
