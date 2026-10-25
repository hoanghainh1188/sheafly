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
   * (empty where `allowEmpty`, else 100%), which the backend also allows; anything else is
   * marked `lockedTitle`.
   */
  locked?: boolean;
  lockedTitle?: string;
  /**
   * Sends the factor. A promise that resolves to nothing means the backend refused it, and
   * the field shows the factor it had again.
   */
  onCommit: (factor: number | null) => unknown;
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
        const input = event.target;
        const commit = (factor: number | null) => {
          const sent = onCommit(factor);
          if (sent instanceof Promise) {
            void sent.then((done) => {
              // An accepted factor remounts the field with the engine's value.
              if (!done) input.value = shown;
            });
          }
        };
        if (text === "") {
          if (allowEmpty && value != null) commit(null);
          else input.value = shown;
          return;
        }
        const factor = parseFocus(text);
        if (factor == null) {
          setBad(badTitle);
          return;
        }
        // A resource's own factor is Pro at any value; only clearing it is free.
        if (locked && (allowEmpty || factor !== 1)) {
          setBad(lockedTitle ?? badTitle);
          return;
        }
        input.value = formatFocus(factor);
        if (factor !== value) commit(factor);
      }}
    />
  );
}
