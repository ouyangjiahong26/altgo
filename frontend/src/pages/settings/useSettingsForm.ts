/** 设置页的装配逻辑：配置表单、模型管理、供应商目录、连通性测试与更新检查。
 * 复用 hooks/ 里的表单与模型钩子，不另造一套配置逻辑。
 * 各区块组件只消费这里的扁平状态与回调。 */
import { useEffect, useState, type Dispatch, type SetStateAction } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../../i18n";
import {
  useConfigForm,
  normalizeConfig,
  type AppConfig,
  type UseConfigFormResult,
} from "../../hooks/useConfigForm";
import { useModelManager, type UseModelManagerResult } from "../../hooks/useModelManager";
import { useTheme } from "../../ThemeContext";
import {
  getFontSizePref,
  getWindowSizePref,
  setFontSizePref,
  setWindowSizePref,
  type FontSizePref,
  type WindowSizePref,
} from "../../ui-size";
import { loadCatalog } from "../../config/catalog";
import type { ProviderPreset } from "../../config/modelPresets";
import { openUpdateNotes, type UpdateInfo } from "../../updateNotes";

/** 设置页全部状态与副作用的唯一入口，Settings 组件与区块树只消费其返回值。 */
export function useSettingsForm() {
  const { t, lang, setLang } = useTranslation();
  const { themePref, setTheme } = useTheme();
  const modelMgr = useModelManager({ t });
  const form = useConfigForm({
    t,
    setLang,
    onAfterSave: () => {
      modelMgr.refreshModels();
    },
  });
  const { config, setConfig, saving, message, update, save, keyCapturing, captureActivationKey } =
    form;
  const { models, downloading, getDownloadProgress } = modelMgr;
  const toggles = useSectionToggles();
  const version = useAppVersion();
  const appearance = useAppearancePrefs();
  const catalog = useOnlineCatalogLoader();
  const connection = usePolisherConnection(config, setConfig);
  const asrKey = useAsrKeyClearing(setConfig);
  const updateCheck = useUpdateChecker();
  const modelActions = useModelActions(form, modelMgr);

  // 预设键不带 evdev 码，换键名时同步清空，避免旧的自定义码继续参与匹配。
  const setKeyName = (keyName: string) => {
    setConfig((prev) =>
      prev
        ? {
            ...prev,
            keyName,
            linuxEvdevCode: null,
          }
        : prev,
    );
  };

  return {
    t, lang, themePref, setTheme,
    config, saving, message, update, save, keyCapturing, captureActivationKey,
    models, downloading, getDownloadProgress, setKeyName,
    ...toggles, ...version, ...appearance, ...catalog,
    ...connection, ...asrKey, ...updateCheck, ...modelActions,
  };
}

/** 各区块折叠开关。本地/在线两个互斥面板的开合放在这里，
 * 切换后端导致面板卸载再挂回后仍保持原开合。 */
function useSectionToggles() {
  const [polishOpen, setPolishOpen] = useState(true);
  const [advancedPath, setAdvancedPath] = useState(false);
  const [asrAdvancedOpen, setAsrAdvancedOpen] = useState(false);
  const [polishAdvancedOpen, setPolishAdvancedOpen] = useState(false);
  return {
    polishOpen, setPolishOpen, advancedPath, setAdvancedPath,
    asrAdvancedOpen, setAsrAdvancedOpen, polishAdvancedOpen, setPolishAdvancedOpen,
  };
}

function useAppVersion() {
  const [appVersion, setAppVersion] = useState<string>("");
  useEffect(() => {
    getVersion().then(setAppVersion).catch(() => {});
  }, []);
  return { appVersion };
}

function useAppearancePrefs() {
  const [fontSize, setFontSize] = useState<FontSizePref>(() => getFontSizePref());
  const [windowSize, setWindowSize] = useState<WindowSizePref>(() => getWindowSizePref());

  // 外观偏好存 localStorage：组件状态与持久化在同一个回调里同步写。
  const handleFontSizeChange = (value: FontSizePref) => {
    setFontSize(value);
    setFontSizePref(value);
  };

  const handleWindowSizeChange = (value: WindowSizePref) => {
    setWindowSize(value);
    setWindowSizePref(value);
  };

  return { fontSize, windowSize, handleFontSizeChange, handleWindowSizeChange };
}

function useOnlineCatalogLoader() {
  const [catalogPresets, setCatalogPresets] = useState<ProviderPreset[]>([]);
  const [catalogLoading, setCatalogLoading] = useState(false);
  const [catalogError, setCatalogError] = useState<string | null>(null);

  // 拉取 omp 供应商目录（设置页挂载时自动调用，失败可重试，手填地址仍可用）。
  const loadOnlineCatalog = async () => {
    if (catalogLoading) return;
    setCatalogLoading(true);
    setCatalogError(null);
    try {
      setCatalogPresets(await loadCatalog());
    } catch (e) {
      setCatalogError(String(e));
    } finally {
      setCatalogLoading(false);
    }
  };

  useEffect(() => {
    void loadOnlineCatalog();
  }, []);

  return { catalogPresets, catalogLoading, catalogError, loadOnlineCatalog };
}

function usePolisherConnection(
  config: AppConfig | null,
  setConfig: Dispatch<SetStateAction<AppConfig | null>>,
) {
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; error?: string } | null>(null);
  const [clearingKey, setClearingKey] = useState(false);

  // 用表单当前值直接测试（密钥留空时后端回落到已存密钥），不必先保存。
  const runTestConnection = async () => {
    if (!config) return;
    setTesting(true);
    setTestResult(null);
    try {
      await invoke("test_polisher_connection", {
        protocol: config.polishProtocol,
        apiBaseUrl: config.polishApiBaseUrl,
        apiKey: config.polisherApiKey,
        model: config.polishModel,
      });
      setTestResult({ ok: true });
    } catch (e) {
      setTestResult({ ok: false, error: String(e) });
    } finally {
      setTesting(false);
    }
  };

  const clearApiKey = async () => {
    setClearingKey(true);
    setTestResult(null);
    try {
      await invoke("save_config", { patch: { polishApiKey: "" } });
      const c = await invoke<AppConfig>("get_config");
      setConfig(normalizeConfig(c));
    } catch (e) {
      setTestResult({ ok: false, error: String(e) });
    } finally {
      setClearingKey(false);
    }
  };

  return { testing, testResult, runTestConnection, clearingKey, clearApiKey };
}

function useAsrKeyClearing(setConfig: Dispatch<SetStateAction<AppConfig | null>>) {
  const [clearingAsrKey, setClearingAsrKey] = useState(false);
  const [asrClearError, setAsrClearError] = useState<string | null>(null);

  // 清除已保存的在线识别密钥（后端存明文，清除后 hasAsrApiKey 变 false）。
  const clearAsrApiKey = async () => {
    setClearingAsrKey(true);
    setAsrClearError(null);
    try {
      await invoke("save_config", { patch: { asrApiKey: "" } });
      const c = await invoke<AppConfig>("get_config");
      setConfig(normalizeConfig(c));
    } catch (e) {
      setAsrClearError(String(e));
    } finally {
      setClearingAsrKey(false);
    }
  };

  return { clearingAsrKey, asrClearError, clearAsrApiKey };
}

function useUpdateChecker() {
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [updateError, setUpdateError] = useState<string | null>(null);

  const handleCheckUpdate = async () => {
    setCheckingUpdate(true);
    setUpdateError(null);
    setUpdateInfo(null);
    try {
      const res = await invoke<UpdateInfo>("check_update", { mode: "manual" });
      setUpdateInfo(res);
      // 手动检查发现新版本即弹更新说明窗口，无更新或失败保持页内提示，不弹窗。
      if (res.hasUpdate) await openUpdateNotes(res);
    } catch (err) {
      // Tauri 命令边界统一 Result<T, String>，拒绝值是错误文案字符串。
      setUpdateError(err instanceof Error ? err.message : String(err));
    } finally {
      setCheckingUpdate(false);
    }
  };

  return { checkingUpdate, updateInfo, updateError, handleCheckUpdate };
}

function useModelActions(form: UseConfigFormResult, modelMgr: UseModelManagerResult) {
  const { config, update, setMessage } = form;

  // 只写模型字段：下载在 model-download-finished 之后才结束，可能晚于用户离开本页。整份回写
  // 会把期间（或在别处）保存的配置覆盖回旧值。窄补丁与清除密钥走同一条路径。
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

  const handleDelete = async (name: string) => {
    await modelMgr.deleteModel(name, (deleted) => {
      if (config?.model === deleted) {
        update("model", "");
      }
    });
  };

  return { applyLocalModel, downloadAndUse, handleDelete };
}
