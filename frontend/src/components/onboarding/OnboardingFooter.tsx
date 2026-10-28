/** 底部操作：次要的“上一步”在左，主操作在右。步骤位置已由头部进度播报，这里不再重复。 */
interface OnboardingFooterProps {
  t: (key: string) => string;
  stepIndex: number;
  saving: boolean;
  nextLabel: string;
  onPrev: () => void;
  onNext: () => void;
}

export function OnboardingFooter({
  t,
  stepIndex,
  saving,
  nextLabel,
  onPrev,
  onNext,
}: OnboardingFooterProps) {
  return (
    <div className="onboarding-foot">
      {/* 第一步没有“上一步”，用 visibility 隐藏而不是移除：主操作位置在换步时不左右跳。 */}
      <button
        type="button"
        className="btn btn-secondary btn-lg"
        onClick={onPrev}
        disabled={saving}
        style={{ visibility: stepIndex === 0 ? "hidden" : "visible" }}
      >
        {t("onboarding.prev")}
      </button>
      <button
        type="button"
        className="btn btn-primary btn-lg"
        onClick={onNext}
        disabled={saving}
      >
        {saving ? t("settings.saving") : nextLabel}
      </button>
    </div>
  );
}
