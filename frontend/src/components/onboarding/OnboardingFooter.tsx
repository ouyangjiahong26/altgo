/** 底部：步骤圆点与上一步/下一步按钮。 */
import { Sparkles } from "lucide-react";
import type { Step } from "./useOnboardingWizard";

interface OnboardingFooterProps {
  t: (key: string) => string;
  steps: Step[];
  stepIndex: number;
  saving: boolean;
  nextLabel: string;
  onPrev: () => void;
  onNext: () => void;
}

export function OnboardingFooter({
  t,
  steps,
  stepIndex,
  saving,
  nextLabel,
  onPrev,
  onNext,
}: OnboardingFooterProps) {
  return (
    <div className="onboarding-foot">
      <div className="onboarding-dots">
        {steps.map((s, i) => (
          <span key={s} className={`onboarding-dot ${i === stepIndex ? "on" : ""}`} />
        ))}
      </div>
      <button
        type="button"
        className="btn btn-secondary"
        onClick={onPrev}
        disabled={saving}
        style={{ visibility: stepIndex === 0 ? "hidden" : "visible" }}
      >
        {t("onboarding.prev")}
      </button>
      <button
        type="button"
        className="btn btn-primary"
        onClick={onNext}
        disabled={saving}
      >
        <Sparkles size={13} />
        {saving ? t("settings.saving") : nextLabel}
      </button>
    </div>
  );
}
