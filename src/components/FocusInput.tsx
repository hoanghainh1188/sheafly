import { useState } from "react";

import { formatFocus, parseFocus } from "../focusPercent";

interface Props {
  id?: string;
  ariaLabel?: string;
  /** The factor shown; `null` shows an empty field (a resource following the project). */
  value: number | null;
  /** What an empty field stands for, shown as its placeholder. */
  placeholder?: string;
  title: string;
  /** The tooltip while the text does not read as 50–100%. */
  badTitle: string;
  /** Empty text commits `null` only when `allowEmpty`; otherwise it puts the value back. */
  allowEmpty?: boolean;
  /**
   * Without Sheafly Pro the field shows the factor and only takes it back to the default
   * (empty, or 100%), which the backend also allows; anything else is marked `lockedTitle`.
   */
  locked?: boolean;
  lockedTitle?: string;
  onCommit: (factor: number | null) => void;
}

/**
 * A productivity factor typed as a percentage, committed on leaving the field. Text that
 * does not read keeps the old factor and is marked, rather than saved as something else.
 */
export function FocusInput({
  id,
  ariaLabel,
  value,
  placeholder,
  title,
  badTitle,
  allowEmpty = false,
  locked = false,
  lockedTitle,
  onCommit,
}: Props) {
  const [bad, setBad] = useState<string | null>(null);
  const shown = value == null ? "" : formatFocus(value);
  return (
    <input
      id={id}
      // An undo or an edit elsewhere shows the engine's factor again.
      key={shown}
      className={bad ? "focus-input bad" : "focus-input"}
      aria-label={ariaLabel}
      aria-invalid={bad != null}
      title={bad ?? (locked && lockedTitle ? `${title}. ${lockedTitle}` : title)}
      placeholder={placeholder}
      defaultValue={shown}
      onChange={() => setBad(null)}
      onBlur={(event) => {
        const text = event.target.value.trim();
        if (text === "") {
          if (allowEmpty && value != null) onCommit(null);
          else event.target.value = shown;
          return;
        }
        const factor = parseFocus(text);
        if (factor == null) {
          setBad(badTitle);
          return;
        }
        if (locked && factor !== 1) {
          setBad(lockedTitle ?? badTitle);
          return;
        }
        event.target.value = formatFocus(factor);
        if (factor !== value) onCommit(factor);
      }}
    />
  );
}
