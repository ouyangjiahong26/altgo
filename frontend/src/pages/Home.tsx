import {
  useStatus,
  useLatestTranscription,
  usePipelineError,
  useKeyListenerBackend,
  useTranscriptionProgress,
} from "../hooks/useTauri";
import { useTranslation } from "../i18n";
import { StatusIndicator } from "../components/StatusIndicator";
import HistoryPanel from "../components/HistoryPanel";
import { Copy, Check } from "lucide-react";
import { useState } from "react";
import { copyToClipboard } from "../utils/clipboard";

export default function Home() {
  const { t } = useTranslation();
  const status = useStatus();
  const transcription = useLatestTranscription();
  const error = usePipelineError();
  const keyBackend = useKeyListenerBackend();
  const txProgress = useTranscriptionProgress();
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    if (transcription) {
      const ok = await copyToClipboard(transcription);
      if (ok) {
        setCopied(true);
        setTimeout(() => setCopied(false), 2000);
      }
    }
  };

  const statusMap: Record<string, 'idle' | 'recording' | 'processing' | 'done'> = {
    idle: 'idle',
    recording: 'recording',
    processing: 'processing',
    done: 'done',
  };

  const mappedStatus = statusMap[status] || 'idle';

  return (
    <div className="home">
      {error && (
        <div className="home-error">
          <span className="error-icon">⚠️</span>
          <p className="error-text">{error}</p>
        </div>
      )}
      {!transcription ? (
        <div className="home-idle">
          <div className="home-status-row">
            <StatusIndicator status={mappedStatus} size="lg" />
            <p className="home-hint">{t("main.hint")}</p>
          </div>
          {mappedStatus === "processing" && (
            <div className="home-tx-progress-wrap">
              <span className="home-tx-progress-phase">
                {txProgress?.phase === "polish"
                  ? t("overlay.polishing")
                  : t("overlay.transcribing")}
              </span>
              <div className="home-tx-progress-bar">
                <div
                  className={`home-tx-progress-fill ${
                    txProgress?.fraction == null ? "indeterminate" : ""
                  }`}
                  style={
                    txProgress?.fraction != null
                      ? {
                          width: `${Math.min(
                            100,
                            Math.max(0, txProgress.fraction * 100)
                          )}%`,
                        }
                      : undefined
                  }
                />
              </div>
            </div>
          )}
          {keyBackend && (
            <p className="home-key-backend">{t(`main.key_backend_${keyBackend}`)}</p>
          )}
        </div>
      ) : (
        <div className="home-result">
          <span className="home-result-label">{t("main.result_label")}</span>
          <div className="home-result-card">
            <p className="home-result-text">{transcription}</p>
          </div>
          <button
            className={`home-copy-btn ${copied ? 'copied' : ''}`}
            onClick={handleCopy}
          >
            {copied ? (
              <>
                <Check size={16} color="var(--color-green)" />
                <span className="home-copy-done">{t("overlay.copied")}</span>
              </>
            ) : (
              <>
                <Copy size={16} />
                <span>{t("main.copy")}</span>
              </>
            )}
          </button>
        </div>
      )}
      <HistoryPanel />
    </div>
  );
}
