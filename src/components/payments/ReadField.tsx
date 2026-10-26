import { useState } from "react";

/** A text cell committed on blur: text that does not read keeps the old value and says why. */
export function ReadField({
  label,
  shown,
  read,
  refusal,
  className,
  title,
  onCommit,
}: {
  label: string;
  shown: string;
  read: (text: string) => string | number | null;
  refusal: string;
  className?: string;
  title?: string;
  onCommit: (value: string | number) => void;
}) {
  const [editing, setEditing] = useState<string | null>(null);
  const [refused, setRefused] = useState(false);
  return (
    <input
      aria-label={label}
      className={className}
      title={refused ? refusal : title}
      aria-invalid={refused}
      value={editing ?? shown}
      onFocus={() => {
        setEditing(shown);
        setRefused(false);
      }}
      onChange={(event) => setEditing(event.target.value)}
      onBlur={() => {
        const text = editing ?? shown;
        setEditing(null);
        if (text.trim() === shown.trim()) return;
        const value = read(text);
        if (value == null) return setRefused(true);
        onCommit(value);
      }}
    />
  );
}
