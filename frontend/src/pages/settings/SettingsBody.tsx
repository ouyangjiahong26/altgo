/** 设置页主体：区块排序容器与各设置区块的编排树，状态全部来自 useSettingsForm。 */
import { Children, isValidElement, type ReactNode } from "react";
import type { useSettingsForm } from "./useSettingsForm";
import { TranscriptionSection } from "./TranscriptionSection";
import { RecordingSection } from "./RecordingSection";
import { PolishingSection } from "./PolishingSection";
import { AppearanceSection } from "./AppearanceSection";
import { LanguageSection } from "./LanguageSection";
import { AboutSection } from "./AboutSection";
import { SaveRow } from "./SaveRow";

type SettingsForm = ReturnType<typeof useSettingsForm>;

function SettingsSectionOrder({ children }: { children: ReactNode }) {
  const sections = Children.toArray(children).sort((a, b) => {
    const orderOf = (node: ReactNode) =>
      isValidElement<{ "data-settings-order"?: number }>(node)
        ? node.props["data-settings-order"] ?? 99
        : 99;
    return orderOf(a) - orderOf(b);
  });

  return <>{sections}</>;
}

export function SettingsBody({ s }: { s: SettingsForm }) {
  const { config } = s;
  // Settings 在配置加载完成前已提前返回，这里只做类型收窄。
  if (!config) return null;

  return (
    <>
      <OrderedSections s={s} />
      <TailSections s={s} />
    </>
  );
}

/** 参与排序的三个区块（录音、转写、润色），渲染顺序由 data-settings-order 决定。 */
function OrderedSections({ s }: { s: SettingsForm }) {
  const { t, config, update } = s;
  if (!config) return null;

  return (
    <SettingsSectionOrder>
      <TranscriptionSection
        data-settings-order={2}
        t={t} config={config} update={update}
        onlineAsr={{
          advancedOpen: s.asrAdvancedOpen, setAdvancedOpen: s.setAsrAdvancedOpen,
          clearingKey: s.clearingAsrKey, clearError: s.asrClearError,
          onClearApiKey: s.clearAsrApiKey,
        }}
        localModel={{
          saving: s.saving, models: s.models, downloading: s.downloading,
          getDownloadProgress: s.getDownloadProgress,
          onUseLocalModel: s.applyLocalModel, onDownloadAndUse: s.downloadAndUse,
          onDeleteModel: s.handleDelete, advancedPath: s.advancedPath,
          setAdvancedPath: s.setAdvancedPath,
        }}
      />
      <RecordingSection
        data-settings-order={1}
        t={t} config={config} onKeyNameChange={s.setKeyName} saving={s.saving}
        keyCapturing={s.keyCapturing} onCaptureActivationKey={s.captureActivationKey}
      />
      <PolishingSection
        data-settings-order={3}
        t={t} lang={s.lang} config={config} update={update}
        catalogPresets={s.catalogPresets} catalogLoading={s.catalogLoading}
        catalogError={s.catalogError} onRetryCatalog={s.loadOnlineCatalog}
        testing={s.testing} testResult={s.testResult} onTestConnection={s.runTestConnection}
        clearingKey={s.clearingKey} onClearApiKey={s.clearApiKey} polishOpen={s.polishOpen}
        setPolishOpen={s.setPolishOpen} polishAdvancedOpen={s.polishAdvancedOpen}
        setPolishAdvancedOpen={s.setPolishAdvancedOpen}
      />
    </SettingsSectionOrder>
  );
}

/** 排序容器之外固定在后面的区块与保存行。 */
function TailSections({ s }: { s: SettingsForm }) {
  const { t, config, update } = s;
  if (!config) return null;

  return (
    <>
      <AppearanceSection
        t={t} config={config} update={update}
        themePref={s.themePref} setTheme={s.setTheme}
        fontSize={s.fontSize} onFontSizeChange={s.handleFontSizeChange}
        windowSize={s.windowSize} onWindowSizeChange={s.handleWindowSizeChange}
      />
      <LanguageSection t={t} config={config} update={update} />
      <AboutSection
        t={t} config={config} update={update}
        appVersion={s.appVersion} checkingUpdate={s.checkingUpdate}
        updateInfo={s.updateInfo} updateError={s.updateError}
        onCheckUpdate={s.handleCheckUpdate}
      />
      <SaveRow t={t} message={s.message} saving={s.saving} onSave={s.save} />
    </>
  );
}
