import { Copy, Download } from "lucide-react";
import { useState } from "react";

import { errorMessage, saveTextReport } from "../../api";
import { useMessages } from "../../i18n";
import type { StatusReport } from "../../types";
import { Button } from "../Button";
import { AdviceText } from "./AdviceLinks";
import { reportBlocks, withNames, type AdviceNames } from "./adviceText";
import { recommendationsMessages } from "./RecommendationsPanel.i18n";

interface StatusReportViewProps {
  report: StatusReport;
  names: AdviceNames;
  onPickTask: (uid: number) => void;
}

/** A file name from the report's title: letters, digits and dashes. */
function fileName(title: string): string {
  const base = title
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .replace(/đ/g, "d")
    .replace(/Đ/g, "D")
    .replace(/[^A-Za-z0-9]+/g, "-")
    .replace(/^-|-$/g, "")
    .slice(0, 60);
  return `${base || "status-report"}.md`;
}

/**
 * Kind 5 (phase 22 PR E): the status report the AI wrote, with tasks and people shown by
 * name. Copy and Save put the names in too, so the text that leaves the app reads without
 * Sheafly; saving goes through the native save dialog as Markdown.
 */
export function StatusReportView({ report, names, onPickTask }: StatusReportViewProps) {
  const m = useMessages(recommendationsMessages);
  const [note, setNote] = useState<{ text: string; bad: boolean } | null>(null);
  const text = withNames(report.markdown, names);
  const show = (value: string) => <AdviceText value={value} names={names} onPickTask={onPickTask} />;

  async function copy() {
    try {
      await navigator.clipboard.writeText(text);
      setNote({ text: m.copied, bad: false });
    } catch (error) {
      setNote({ text: m.copyFailed(errorMessage(error)), bad: true });
    }
  }

  async function save() {
    try {
      await saveTextReport(text, fileName(withNames(report.title, names)));
      setNote(null);
    } catch (error) {
      setNote({ text: errorMessage(error), bad: true });
    }
  }

  return (
    <article className="ai-report" aria-label={m.reportHeading}>
      <div className="ai-panel-actions">
        <Button Icon={Copy} onClick={() => void copy()}>
          {m.copy}
        </Button>
        <Button Icon={Download} onClick={() => void save()}>
          {m.saveMarkdown}
        </Button>
      </div>
      {note && (
        <p role={note.bad ? "alert" : "status"} className={note.bad ? "ai-panel-error" : "evm-note"}>
          {note.text}
        </p>
      )}
      {reportBlocks(report.markdown).map((block, at) =>
        block.kind === "heading" ? (
          block.level === 1 ? (
            <h3 key={at}>{show(block.text)}</h3>
          ) : (
            <h4 key={at}>{show(block.text)}</h4>
          )
        ) : block.kind === "list" ? (
          <ul key={at}>
            {block.items.map((item, index) => (
              <li key={index}>{show(item)}</li>
            ))}
          </ul>
        ) : (
          <p key={at}>{show(block.text)}</p>
        ),
      )}
    </article>
  );
}
