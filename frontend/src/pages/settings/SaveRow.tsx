import { Check, Loader2, Save } from "lucide-react";

interface SaveRowProps {
  t: (key: string) => string;
  message: string;
  saving: boolean;
  onSave: () => Promise<void>;
}

/**
 * 保存动作卡：独立一张卡，跟在表单末尾随内容滚动，不吸在窗口最底，
 * 因此不会和浮动底栏叠在一起。保存中按钮自身给出进度，成功与失败落在左侧状态区。
 */
export function SaveRow({ t, message, saving, onSave }: SaveRowProps) {
  return (
    <div className="card settings-save" aria-busy={saving}>
      <span className="settings-save-status" role="status" aria-live="polite">
        {message === "saved" && (
          <span className="badge badge-success">
            <Check size={11} aria-hidden="true" />
            {t("settings.saved")}
          </span>
        )}
        {message && message !== "saved" && (
          <span className="settings-save-error">{message}</span>
        )}
      </span>
      <button
        type="button"
        className="btn btn-primary"
        onClick={onSave}
        disabled={saving}
      >
        {saving ? (
          <Loader2 size={13} className="settings-spin" aria-hidden="true" />
        ) : (
          <Save size={13} aria-hidden="true" />
        )}
        {saving ? t("settings.saving") : t("settings.save")}
      </button>
    </div>
  );
}
