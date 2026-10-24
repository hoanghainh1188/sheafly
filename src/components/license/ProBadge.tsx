import { useMessages } from "../../i18n";
import { proBadgeMessages } from "./ProBadge.i18n";

/**
 * A small "Pro" mark beside a feature that needs Sheafly Pro. It shows on every build,
 * licensed or not, so people can see which features are paid; the backend still decides
 * what runs. Its text joins the name of the control it sits in, after a space.
 */
export function ProBadge() {
  const m = useMessages(proBadgeMessages);
  // The space keeps the name of a header or label readable ("Năng suất Pro").
  return (
    <>
      {" "}
      <span className="pro-badge" title={m.title}>
        {m.label}
      </span>
    </>
  );
}
