import { Check, Save } from "lucide-react";

interface SaveRowProps {
  t: (key: string) => string;
  message: string;
  saving: boolean;
  onSave: () => Promise<void>;
}

/** 设置页底部保存行：回显保存/错误消息并触发整表单保存。 */
export function SaveRow({ t, message, saving, onSave }: SaveRowProps) {
  return (
    <div className="settings-save-row">
      {message === "saved" && (
        <span className="settings-save-msg settings-save-msg--ok">
          <Check size={12} /> {t("settings.saved")}
        </span>
      )}
      {message && message !== "saved" && (
        <span className="settings-save-msg settings-save-msg--err">{message}</span>
      )}
      <button
        type="button"
        className="btn btn-primary"
        onClick={onSave}
        disabled={saving}
      >
        <Save size={13} />
        {saving ? t("settings.saving") : t("settings.save")}
      </button>
    </div>
  );
}
