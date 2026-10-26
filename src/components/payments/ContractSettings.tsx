import { formatDateOnly } from "../../format";
import { useMessages } from "../../i18n";
import type { CashFlowSettings, PaymentDates, Project } from "../../types";
import { MoneyInput } from "../MoneyInput";
import { AnchorPicker } from "./AnchorPicker";
import { ReadField } from "./ReadField";
import { DEFAULT_LAG_DAYS, MAX_RETENTION_PERCENT, defaultAnchor, formatPercent, parseLagDays, parsePercent } from "./paymentText";
import { paymentMessages } from "./payments.i18n";

interface Props {
  project: Project;
  settings: CashFlowSettings;
  /** When the retention is released and due, as scheduled now. */
  releaseDates: PaymentDates | null | undefined;
  /** Applied to the settings sent last, so quick edits in a row all count. */
  onChange: (update: (settings: CashFlowSettings) => CashFlowSettings) => void;
}

/** The contract value, opening balance and retention of the project's cash flow. */
export function ContractSettings({ project, settings, releaseDates, onChange }: Props) {
  const m = useMessages(paymentMessages);
  const retention = settings.retention;
  return (
    <fieldset className="payment-contract">
      <legend>{m.contract}</legend>
      <label title={m.contractValueTip}>
        <span>{m.contractValue}</span>
        <MoneyInput
          ariaLabel={m.contractValue}
          title={m.contractValueTip}
          optional
          value={settings.contract_value}
          onCommit={(value) => onChange((last) => ({ ...last, contract_value: value }))}
        />
      </label>
      <label title={m.openingBalanceTip}>
        <span>{m.openingBalance}</span>
        <MoneyInput
          ariaLabel={m.openingBalance}
          title={m.openingBalanceTip}
          signed
          value={settings.opening_balance}
          onCommit={(value) => onChange((last) => ({ ...last, opening_balance: value ?? 0 }))}
        />
      </label>
      <label className="payment-check-label" title={m.retentionTip}>
        <input
          type="checkbox"
          checked={retention != null}
          onChange={(event) =>
            onChange((last) => ({
              ...last,
              retention: event.target.checked
                ? { percent: 5, release: defaultAnchor(project), lag_days: DEFAULT_LAG_DAYS }
                : null,
            }))
          }
        />
        <span>{m.retention}</span>
      </label>
      {retention && (
        <div className="payment-retention">
          <label>
            <span>{m.retentionPercent}</span>
            <ReadField
              className="narrow"
              label={m.retentionPercent}
              shown={formatPercent(retention.percent)}
              read={(text) => parsePercent(text, MAX_RETENTION_PERCENT)}
              refusal={m.retentionUnreadable}
              onCommit={(percent) => onChange((last) => ({ ...last, retention: last.retention && { ...last.retention, percent: Number(percent) } }))}
            />
          </label>
          <label>
            <span>{m.retentionRelease}</span>
            <AnchorPicker
              project={project}
              anchor={retention.release}
              label={m.retentionRelease}
              onChange={(release) => onChange((last) => ({ ...last, retention: last.retention && { ...last.retention, release } }))}
            />
          </label>
          <label title={m.lagTip}>
            <span>{m.lag}</span>
            <ReadField
              className="narrow"
              label={`${m.retentionRelease}: ${m.lag}`}
              shown={String(retention.lag_days)}
              read={parseLagDays}
              refusal={m.lagUnreadable}
              onCommit={(lag_days) => onChange((last) => ({ ...last, retention: last.retention && { ...last.retention, lag_days: Number(lag_days) } }))}
            />
          </label>
          <span className="meta">
            {m.invoice}: {releaseDates ? formatDateOnly(releaseDates.invoice) : m.noDate} · {m.due}:{" "}
            {releaseDates ? formatDateOnly(releaseDates.due) : m.noDate}
          </span>
        </div>
      )}
    </fieldset>
  );
}
