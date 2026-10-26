import { useEffect, useRef, useState } from "react";

import { cashFlow, errorMessage } from "../../api";
import type { Bucket, CashFlowReport, Project } from "../../types";

export interface CashFlowState {
  report: CashFlowReport | null;
  /** The report is for the project as it is now. */
  current: boolean;
  error: string | null;
}

/**
 * Asks the engine for the cash flow whenever the project or the options change, while
 * `enabled` (the view is open and licensed). The last report stays, marked not current,
 * until the next one comes; answers to older requests are dropped.
 */
export function useCashFlow(project: Project, enabled: boolean, bucket: Bucket, baseline: number | null): CashFlowState {
  const [state, setState] = useState<CashFlowState>({ report: null, current: false, error: null });
  const request = useRef(0);
  useEffect(() => {
    if (!enabled) return;
    const id = ++request.current;
    setState((last) => ({ ...last, current: false }));
    cashFlow({ bucket, baseline })
      .then((report) => {
        if (id === request.current) setState({ report, current: true, error: null });
      })
      .catch((error: unknown) => {
        if (id === request.current) setState({ report: null, current: true, error: errorMessage(error) });
      });
  }, [project, enabled, bucket, baseline]);
  return state;
}
