/** 向导主体：按当前步骤渲染对应的步骤组件。 */
import type { useOnboardingWizard } from "./useOnboardingWizard";
import { WelcomeStep } from "./WelcomeStep";
import { KeyStep } from "./KeyStep";
import { EngineStep } from "./EngineStep";
import { PolishStep } from "./PolishStep";
import { DoneStep } from "./DoneStep";

type Wizard = ReturnType<typeof useOnboardingWizard>;

export function OnboardingBody({ w }: { w: Wizard }) {
  const { t, config, step } = w;
  // 向导只在配置加载完成后渲染主体，这里只做类型收窄。
  if (!config) return null;

  return (
    <div className="onboarding-body">
      {step === "welcome" && <WelcomeStep t={t} />}
      {step === "key" && (
        <KeyStep
          t={t}
          stepLabel={w.stepLabel}
          config={config}
          setConfig={w.setConfig}
          saving={w.saving}
          keyCapturing={w.keyCapturing}
          captureActivationKey={w.captureActivationKey}
        />
      )}
      {step === "engine" && (
        <EngineStep
          t={t}
          stepLabel={w.stepLabel}
          config={config}
          update={w.update}
          saving={w.saving}
          models={w.models}
          downloading={w.downloading}
          getDownloadProgress={w.getDownloadProgress}
          applyLocalModel={w.applyLocalModel}
          downloadAndUse={w.downloadAndUse}
        />
      )}
      {step === "polish" && (
        <PolishStep
          t={t}
          lang={w.lang}
          stepLabel={w.stepLabel}
          config={config}
          update={w.update}
          catalogPresets={w.catalogPresets}
          catalogError={w.catalogError}
        />
      )}
      {step === "done" && (
        <DoneStep
          t={t}
          config={config}
          onlineActive={w.onlineActive}
          message={w.message}
        />
      )}
    </div>
  );
}
