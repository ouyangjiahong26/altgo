/** 首次安装引导：五步向导（欢迎 → 触发键 → 转写引擎 → 润色 → 完成）。
 * 状态与副作用在 onboarding/useOnboardingWizard，各步界面在 onboarding/ 目录。 */
import { useOnboardingWizard } from "./onboarding/useOnboardingWizard";
import { OnboardingHeader } from "./onboarding/OnboardingHeader";
import { OnboardingBody } from "./onboarding/OnboardingBody";
import { OnboardingFooter } from "./onboarding/OnboardingFooter";

export default function Onboarding({ onDone }: { onDone: () => void }) {
  const w = useOnboardingWizard(onDone);
  const { t, config } = w;

  // 配置还没读回来：整窗先立起来，只留一句加载文案，避免引导期间闪一屏主界面。
  if (!config) {
    return (
      <div className="onboarding">
        <div className="onboarding-body onboarding-body--loading">
          {t("settings.loading")}
        </div>
      </div>
    );
  }

  return (
    <div className="onboarding">
      <OnboardingHeader step={w.step} stepLabel={w.stepLabel} onDragStart={w.startDragging} />
      <OnboardingBody w={w} />
      <OnboardingFooter
        t={t}
        stepIndex={w.stepIndex}
        saving={w.saving}
        nextLabel={w.nextLabel}
        onPrev={() => w.goto(w.stepIndex - 1)}
        onNext={w.handleNext}
      />
    </div>
  );
}
