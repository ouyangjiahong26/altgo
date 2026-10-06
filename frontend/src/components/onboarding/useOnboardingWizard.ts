/** 首次安装引导向导的装配逻辑：步骤推进、配置表单、模型下载与收尾保存。
 * 复用设置页的表单钩子、模型管理与供应商选择器，不另造一套配置逻辑。 */
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useTranslation } from "../../i18n";
import {
  useConfigForm,
  type UseConfigFormResult,
} from "../../hooks/useConfigForm";
import {
  useModelManager,
  type UseModelManagerResult,
} from "../../hooks/useModelManager";
import { loadCatalog } from "../../config/catalog";
import { type ProviderPreset } from "../../config/modelPresets";
import { completeOnboarding } from "../../onboarding";

export type Step = "welcome" | "key" | "engine" | "polish" | "done";

export const STEPS: Step[] = ["welcome", "key", "engine", "polish", "done"];

/** Onboarding 组件只按步骤渲染，向导全部状态与副作用集中在这里。 */
export function useOnboardingWizard(onDone: () => void) {
  const { t, lang, setLang } = useTranslation();

  const modelMgr = useModelManager({ t });
  const form = useConfigForm({
    t,
    setLang,
    onAfterSave: () => {
      modelMgr.refreshModels();
    },
  });
  const {
    config, setConfig, saving, message, setMessage, update, save,
    keyCapturing, captureActivationKey,
  } = form;
  const { models, downloading, getDownloadProgress } = modelMgr;

  const catalog = useOnboardingCatalog();
  const { finish } = useOnboardingFinish(saving, message, onDone, save);
  const modelActions = useOnboardingModelActions(update, setMessage, modelMgr);
  const nav = useStepNavigation(() => void finish());

  const onlineActive = config?.transcriberBackend === "online";
  const stepLabel = (n: number) => stepLabelText(t, n);
  const nextLabel = nextStepLabelText(t, nav.step, config?.polishLevel);

  return {
    t, lang, config, setConfig, saving, message, update,
    keyCapturing, captureActivationKey,
    models, downloading, getDownloadProgress,
    catalogPresets: catalog.catalogPresets, catalogError: catalog.catalogError,
    step: nav.step, stepIndex: nav.stepIndex, onlineActive,
    stepLabel, nextLabel, goto: nav.goto, handleNext: nav.handleNext,
    applyLocalModel: modelActions.applyLocalModel,
    downloadAndUse: modelActions.downloadAndUse,
    startDragging,
  };
}

/** 供应商目录：向导挂载时拉取一次，失败记错误（仍可手填）。 */
function useOnboardingCatalog() {
  const [catalogPresets, setCatalogPresets] = useState<ProviderPreset[]>([]);
  const [catalogError, setCatalogError] = useState<string | null>(null);

  useEffect(() => {
    loadCatalog()
      .then(setCatalogPresets)
      .catch((e) => setCatalogError(String(e)));
  }, []);

  return { catalogPresets, catalogError };
}

/** 收尾保存：保存成功后落引导标记并结束向导，失败留在完成步显示表单错误。 */
function useOnboardingFinish(
  saving: boolean,
  message: string,
  onDone: () => void,
  save: () => Promise<void>,
) {
  const [finishing, setFinishing] = useState(false);

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

  const finish = async () => {
    setFinishing(true);
    await save();
  };

  return { finish };
}

/** 步骤推进：索引夹在 STEPS 范围内，完成步的“下一步”触发收尾保存。 */
function useStepNavigation(onFinish: () => void) {
  const [stepIndex, setStepIndex] = useState(0);
  const step = STEPS[stepIndex];

  const goto = (index: number) => {
    setStepIndex(Math.max(0, Math.min(STEPS.length - 1, index)));
  };

  const handleNext = () => {
    if (step === "done") {
      onFinish();
      return;
    }
    goto(stepIndex + 1);
  };

  return { step, stepIndex, goto, handleNext };
}

/** 模型下载与应用动作。 */
function useOnboardingModelActions(
  update: UseConfigFormResult["update"],
  setMessage: UseConfigFormResult["setMessage"],
  modelMgr: UseModelManagerResult,
) {
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

  return { applyLocalModel, downloadAndUse };
}

/** 步号只数配置步（触发键 / 转写引擎 / 润色，共 3 步）。欢迎页与完成页不编号。 */
function stepLabelText(t: (key: string) => string, n: number) {
  return t("onboarding.step_label").replace("{n}", String(n));
}

/** 下一步按钮文案：完成步是“开始使用”，润色档位为关闭时是“跳过”。 */
function nextStepLabelText(
  t: (key: string) => string,
  step: Step,
  polishLevel: string | undefined,
) {
  return step === "done"
    ? t("onboarding.start")
    : step === "polish" && polishLevel === "none"
      ? t("onboarding.polish_skip")
      : t("onboarding.next");
}

/** 拖拽区按下即整体拖动窗口；非 Tauri 环境静默忽略。 */
function startDragging() {
  try {
    getCurrentWindow().startDragging();
  } catch {
    // 忽略
  }
}
