/** 首次安装引导：五步向导（欢迎 → 触发键 → 转写引擎 → 润色 → 完成）。
 * 状态与副作用在 onboarding/useOnboardingWizard，各步界面在 onboarding/ 目录。 */
import { STEPS, useOnboardingWizard } from "./onboarding/useOnboardingWizard";
import { OnboardingBody } from "./onboarding/OnboardingBody";
import { OnboardingFooter } from "./onboarding/OnboardingFooter";

export default function Onboarding({ onDone }: { onDone: () => void }) {
  const w = useOnboardingWizard(onDone);
  const { t, config } = w;

  if (!config) {
    return (
      <div className="onboarding">
        <div className="onboarding-body">{t("settings.loading")}</div>
      </div>
    );
  }

  return (
    <div className="onboarding">
      <div className="onboarding-drag" data-tauri-drag-region onMouseDown={w.startDragging} />
      <OnboardingBody w={w} />
      <OnboardingFooter
        t={t}
        steps={STEPS}
        stepIndex={w.stepIndex}
        saving={w.saving}
        nextLabel={w.nextLabel}
        onPrev={() => w.goto(w.stepIndex - 1)}
        onNext={w.handleNext}
      />
    </div>
  );
}
