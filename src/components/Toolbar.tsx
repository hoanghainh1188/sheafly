import {
  ChartLine,
  Diamond,
  FileDown,
  FolderCog,
  IndentDecrease,
  IndentIncrease,
  Link2,
  ListPlus,
  Plus,
  Redo2,
  Repeat,
  Scissors,
  Sparkles,
  Trash2,
  Undo2,
  Unlink2,
  UserCheck,
  type LucideIcon,
} from "lucide-react";
import type { ButtonHTMLAttributes } from "react";

import type { ToolbarStates } from "../toolbarState";
import type { DepKind } from "../types";
import { useMessages } from "../i18n";
import { ProBadge } from "./license/ProBadge";
import { MenuButton } from "./MenuButton";
import { toolbarMessages } from "./Toolbar.i18n";

export interface ToolbarActions {
  onUndo: () => void;
  onRedo: () => void;
  onAddTask: () => void;
  onAddMilestone: () => void;
  onAddRecurring: () => void;
  onIndent: () => void;
  onOutdent: () => void;
  onLink: (kind: DepKind) => void;
  onUnlink: () => void;
  onSplit: () => void;
  onDelete: () => void;
  onProjectInfo: () => void;
  onAssignByRules: () => void;
  onToggleEvm: () => void;
  onToggleAi: () => void;
  onExportPdf: () => void;
}

interface Props extends ToolbarActions {
  states: ToolbarStates;
  evmOpen: boolean;
  aiOpen: boolean;
}

/**
 * Each group has its own colour, so the eye finds a command by where it belongs:
 * history gray, creating green, editing the selected task blue, deleting red, the
 * project purple, reports amber.
 */
type Tone = "history" | "create" | "edit" | "danger" | "project" | "report";

const LINKS: { kind: DepKind; label: "linkFs" | "linkSs" | "linkFf" | "linkSf" }[] = [
  { kind: "fs", label: "linkFs" },
  { kind: "ss", label: "linkSs" },
  { kind: "ff", label: "linkFf" },
  { kind: "sf", label: "linkSf" },
];

function icon(Icon: LucideIcon) {
  return <Icon aria-hidden="true" size={15} strokeWidth={1.9} />;
}

interface ToolProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  tone: Tone;
  Icon: LucideIcon;
  label: string;
  /** A Sheafly Pro feature: shows the Pro badge after the label. */
  pro?: boolean;
}

function Tool({ tone, Icon, label, pro, className, ...rest }: ToolProps) {
  return (
    <button type="button" className={["tool", `tone-${tone}`, className].filter(Boolean).join(" ")} {...rest}>
      {icon(Icon)}
      <span>
        {label}
        {pro && <ProBadge />}
      </span>
    </button>
  );
}

/**
 * Everyday commands in groups, left to right: history, creating tasks, editing the
 * selected task, the project, and reports. System commands and shortcuts stay in the
 * macOS menu. A command that cannot run is disabled with its reason as the tooltip.
 */
export function Toolbar({ states, evmOpen, aiOpen, ...actions }: Props) {
  const m = useMessages(toolbarMessages);
  const tip = (key: keyof ToolbarStates, fallback: string) => states[key].reason ?? fallback;
  return (
    <header className="toolbar">
      <strong className="toolbar-brand">Sheafly</strong>
      <div className="toolbar-group" role="group" aria-label={m.history}>
        <Tool
          tone="history"
          Icon={Undo2}
          label={m.undo}
          disabled={!states.undo.enabled}
          title={tip("undo", m.undoTip)}
          onClick={actions.onUndo}
        />
        <Tool
          tone="history"
          Icon={Redo2}
          label={m.redo}
          disabled={!states.redo.enabled}
          title={tip("redo", m.redoTip)}
          onClick={actions.onRedo}
        />
      </div>
      <div className="toolbar-group" role="group" aria-label={m.create}>
        <MenuButton
          label={m.add}
          className="tool tone-create primary"
          icon={icon(Plus)}
          disabled={!states.add.enabled}
          title={tip("add", m.addTip)}
          items={[
            { label: m.addTask, icon: icon(ListPlus), onSelect: actions.onAddTask },
            { label: m.addMilestone, icon: icon(Diamond), onSelect: actions.onAddMilestone },
            { label: m.addRecurring, icon: icon(Repeat), onSelect: actions.onAddRecurring },
          ]}
        />
      </div>
      <div className="toolbar-group" role="group" aria-label={m.selected}>
        <Tool
          tone="edit"
          Icon={IndentDecrease}
          label={m.outdent}
          disabled={!states.outdent.enabled}
          title={tip("outdent", m.outdentTip)}
          onClick={actions.onOutdent}
        />
        <Tool
          tone="edit"
          Icon={IndentIncrease}
          label={m.indent}
          disabled={!states.indent.enabled}
          title={tip("indent", m.indentTip)}
          onClick={actions.onIndent}
        />
        <MenuButton
          label={m.link}
          className="tool tone-edit"
          icon={icon(Link2)}
          disabled={!states.link.enabled && !states.unlink.enabled}
          title={states.link.enabled || states.unlink.enabled ? m.linkTip : tip("link", "")}
          items={[
            ...LINKS.map(({ kind, label }) => ({
              label: m[label],
              icon: icon(Link2),
              onSelect: () => actions.onLink(kind),
              disabled: !states.link.enabled,
              title: states.link.reason,
            })),
            {
              label: m.unlink,
              icon: icon(Unlink2),
              onSelect: actions.onUnlink,
              disabled: !states.unlink.enabled,
              title: states.unlink.reason,
            },
          ]}
        />
        <Tool
          tone="edit"
          Icon={Scissors}
          label={m.split}
          disabled={!states.split.enabled}
          title={tip("split", m.splitTip)}
          onClick={actions.onSplit}
        />
        <Tool
          tone="danger"
          Icon={Trash2}
          label={m.remove}
          disabled={!states.remove.enabled}
          title={tip("remove", m.removeTip)}
          onClick={actions.onDelete}
        />
      </div>
      <div className="toolbar-group" role="group" aria-label={m.project}>
        <Tool
          tone="project"
          Icon={FolderCog}
          label={m.projectInfo}
          disabled={!states.project.enabled}
          title={tip("project", m.projectInfoTip)}
          onClick={actions.onProjectInfo}
        />
        <Tool
          tone="project"
          Icon={UserCheck}
          label={m.assignByRules}
          disabled={!states.assignByRules.enabled}
          title={tip("assignByRules", m.assignByRulesTip)}
          onClick={actions.onAssignByRules}
        />
      </div>
      <span className="spacer" />
      <div className="toolbar-group" role="group" aria-label={m.reports}>
        <Tool
          tone="report"
          Icon={ChartLine}
          label={m.evm}
          className={evmOpen ? "on" : undefined}
          aria-pressed={evmOpen}
          disabled={!states.evm.enabled}
          title={tip("evm", m.evmTip)}
          onClick={actions.onToggleEvm}
        />
        <Tool
          tone="report"
          Icon={Sparkles}
          label={m.ai}
          pro
          className={aiOpen ? "on" : undefined}
          aria-pressed={aiOpen}
          disabled={!states.ai.enabled}
          title={tip("ai", m.aiTip)}
          onClick={actions.onToggleAi}
        />
        <Tool
          tone="report"
          Icon={FileDown}
          label={m.exportPdf}
          disabled={!states.exportPdf.enabled}
          title={tip("exportPdf", m.exportPdfTip)}
          onClick={actions.onExportPdf}
        />
      </div>
    </header>
  );
}
