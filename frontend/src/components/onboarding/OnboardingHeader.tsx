/** 向导头部：品牌、步骤编号与进度条。整条可拖拽，引导期间窗口没有系统标题栏。 */
import type { Step } from "./useOnboardingWizard";

/** 配置步的编号：欢迎与完成不编号，与 i18n 的“第 n 步 / 共 3 步”对应。 */
const CONFIG_STEP: Partial<Record<Step, number>> = { key: 1, engine: 2, polish: 3 };

/** 进度条段数：等于配置步数，欢迎与完成步不额外占段。 */
const SEGMENTS = [1, 2, 3];

interface OnboardingHeaderProps {
  step: Step;
  stepLabel: (n: number) => string;
  onDragStart: () => void;
}

export function OnboardingHeader({ step, stepLabel, onDragStart }: OnboardingHeaderProps) {
  const current = CONFIG_STEP[step];
  // 走到第几步就点亮几段；完成步三段全亮，欢迎步一段不亮。
  const filled = step === "done" ? SEGMENTS.length : (current ?? 0);

  return (
    <>
      <header className="onboarding-header" data-tauri-drag-region onMouseDown={onDragStart}>
        <span className="onboarding-brand">
          <img
            src="/altgo-logo.svg"
            alt=""
            width={18}
            height={18}
            className="onboarding-brand-mark"
          />
          <span className="onboarding-brand-name">altgo</span>
        </span>
        {current != null && (
          <span className="onboarding-step-label">{stepLabel(current)}</span>
        )}
      </header>
      {/* 纯视觉进度：步骤文字已由右侧标签播报，重复朗读没有意义。 */}
      <div className="onboarding-progress" aria-hidden>
        {SEGMENTS.map((n) => (
          <span
            key={n}
            className={`onboarding-progress-seg ${n <= filled ? "is-done" : ""}`}
          />
        ))}
      </div>
    </>
  );
}
