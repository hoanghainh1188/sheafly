import { useId, useState } from "react";

import { formatMoneyInput, parseMoney } from "../format";
import { useLang, useMessages } from "../i18n";
import { moneyInputMessages } from "./MoneyInput.i18n";

interface Props {
  value: number | null;
  onCommit: (value: number | null) => void;
  /** Empty sends null (back to the computed amount); otherwise empty is refused. */
  optional?: boolean;
  /** A leading minus is read as a negative amount (a balance), not refused. */
  signed?: boolean;
  ariaLabel?: string;
  title?: string;
  placeholder?: string;
  className?: string;
  disabled?: boolean;
}

/**
 * A money amount: shown with the language's grouping ("1.248.000"), read back grouped or
 * plain with either decimal mark (`parseMoney`). Text that does not read keeps the old
 * amount and says why, rather than saving 0.
 */
export function MoneyInput({ value, onCommit, optional, signed, ariaLabel, title, placeholder, className, disabled }: Props) {
  const m = useMessages(moneyInputMessages);
  const lang = useLang();
  const shown = value == null ? "" : formatMoneyInput(value);
  const [editing, setEditing] = useState<string | null>(null);
  const [refused, setRefused] = useState<string | null>(null);
  const noteId = useId();
  return (
    <>
      <input
        key={lang}
        aria-label={ariaLabel}
        title={refused ?? title}
        placeholder={placeholder}
        className={className}
        disabled={disabled}
        inputMode="decimal"
        aria-invalid={refused != null}
        aria-describedby={refused ? noteId : undefined}
        value={editing ?? shown}
        onFocus={() => {
          setEditing(shown);
          setRefused(null);
        }}
        onChange={(event) => setEditing(event.target.value)}
        onBlur={() => {
          const text = (editing ?? "").trim();
          setEditing(null);
          if (!text) {
            if (!optional) return setRefused(m.required);
            if (value != null) onCommit(null);
            return;
          }
          const negative = signed === true && /^[-−]/.test(text);
          const read = parseMoney(negative ? text.slice(1) : text);
          const amount = read == null ? null : negative ? -read : read;
          if (amount == null) return setRefused(m.unreadable);
          if (amount !== value) onCommit(amount);
        }}
      />
      {refused && (
        <span id={noteId} hidden>
          {refused}
        </span>
      )}
    </>
  );
}
