/** 首次安装引导向导的装配逻辑：步骤推进、配置表单、模型下载与收尾保存。
 * 复用设置页的表单钩子、模型管理与供应商选择器，不另造一套配置逻辑。 */
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useTranslation } from "../../i18n";
import { useConfigForm } from "../../hooks/useConfigForm";
import { useModelManager } from "../../hooks/useModelManager";
import { loadCatalog } from "../../config/catalog";
import { type ProviderPreset } from "../../config/modelPresets";
import { completeOnboarding } from "../../onboarding";

export type Step = "welcome" | "key" | "engine" | "polish" | "done";

export const STEPS: Step[] = ["welcome", "key", "engine", "polish", "done"];

/** Onboarding 组件只按步骤渲染，向导全部状态与副作用集中在这里。 */
export function useOnboardingWizard(onDone: () => void) {
  const { t, lang, setLang } = useTranslation();
  const [stepIndex, setStepIndex] = useState(0);
  const [finishing, setFinishing] = useState(false);
  const [catalogPresets, setCatalogPresets] = useState<ProviderPreset[]>([]);
  const [catalogError, setCatalogError] = useState<string | null>(null);

  const modelMgr = useModelManager({ t });
  const form = useConfigForm({
    t,
    setLang,
    onAfterSave: () => {
      modelMgr.refreshModels();
    },
  });
  const {
    config,
    setConfig,
    saving,
    message,
    setMessage,
    update,
    save,
    keyCapturing,
    captureActivationKey,
  } = form;
  const { models, downloading, getDownloadProgress } = modelMgr;

  const step = STEPS[stepIndex];
  const onlineActive = config?.transcriberBackend === "online";

  useEffect(() => {
    loadCatalog()
      .then(setCatalogPresets)
      .catch((e) => setCatalogError(String(e)));
  }, []);

  // 保存成功后收尾，保存失败则留在完成步并显示表单错误。
  useEffect(() => {
    if (!finishing || saving) return;
    if (message === "saved") {
      completeOnboarding();
      onDone();
    } else if (message !== "") {
      setFinishing(false);
    }
  }, [finishing, saving, message, onDone]);

  const goto = (index: number) => {
    setStepIndex(Math.max(0, Math.min(STEPS.length - 1, index)));
  };

  // 只写模型字段：下载在 model-download-finished 之后才结束，可能晚于向导关闭，整份回写会把
  // 期间（含向导结束后在设置页）保存的配置覆盖回旧值。
  const applyLocalModel = async (name: string) => {
    update("model", name);
    try {
      await invoke("save_config", { patch: { model: name } });
    } catch (e) {
      setMessage(String(e));
    }
  };

  const downloadAndUse = async (name: string) => {
    await modelMgr.downloadAndUse(name, applyLocalModel);
  };

  const finish = async () => {
    setFinishing(true);
    await save();
  };

  const startDragging = () => {
    try {
      getCurrentWindow().startDragging();
    } catch {
      // 忽略
    }
  };

  /** 步号只数配置步（触发键 / 转写引擎 / 润色，共 3 步）。欢迎页与完成页不编号。 */
  const stepLabel = (n: number) =>
    t("onboarding.step_label").replace("{n}", String(n));

  const nextLabel =
    step === "done"
      ? t("onboarding.start")
      : step === "polish" && config?.polishLevel === "none"
        ? t("onboarding.polish_skip")
        : t("onboarding.next");

  const handleNext = () => {
    if (step === "done") {
      void finish();
      return;
    }
    goto(stepIndex + 1);
  };

  return {
    t,
    lang,
    config,
    setConfig,
    saving,
    message,
    update,
    keyCapturing,
    captureActivationKey,
    models,
    downloading,
    getDownloadProgress,
    catalogPresets,
    catalogError,
    step,
    stepIndex,
    onlineActive,
    stepLabel,
    nextLabel,
    goto,
    handleNext,
    applyLocalModel,
    downloadAndUse,
    startDragging,
  };
}
