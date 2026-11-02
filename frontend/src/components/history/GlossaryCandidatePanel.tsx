/** 历史条目的候选术语面板：列出 AI 建议、允许手动补充，并把选中的词交给上层写入术语表。 */
import { AlertCircle, Loader2 } from "lucide-react";
import { GlossaryChipEditor } from "../GlossaryChipEditor";
import {
  MAX_GLOSSARY_TERMS,
  type SkippedGlossaryTerm,
} from "../../utils/glossary";
import type { HistoryEntry } from "./useHistoryPanel";
import type {
  GlossaryCandidateController,
  GlossaryCandidateInfo,
} from "./useGlossaryCandidates";

interface GlossaryCandidatePanelProps {
  t: (key: string) => string;
  entry: HistoryEntry;
  candidate: GlossaryCandidateController;
}

export function GlossaryCandidatePanel({
  t,
  entry,
  candidate,
}: GlossaryCandidatePanelProps) {
  return (
    <div
      className="history-candidate-panel"
      role="region"
      aria-label={t("history.glossary_candidates_open")}
    >
      <CandidateFeedback t={t} entry={entry} candidate={candidate} />

      <CandidateOptions
        t={t}
        candidates={candidate.candidates}
        selected={candidate.selected}
        disabled={candidate.adding}
        onToggle={candidate.toggle}
      />

      {candidate.addedCount > 0 && (
        <p className="history-candidate-success" role="status">
          {t("history.glossary_candidates_added").replace(
            "{n}",
            String(candidate.addedCount),
          )}
        </p>
      )}

      <SkippedTerms t={t} skipped={candidate.skipped} />

      <div className="history-candidate-manual">
        <GlossaryChipEditor
          t={t}
          terms={candidate.manualTerms}
          input={candidate.manualInput}
          placeholder={t("history.glossary_candidates_manual_placeholder")}
          disabled={candidate.adding}
          onInputChange={candidate.setManualInput}
          onAdd={candidate.addManual}
          onRemove={candidate.removeManual}
        />
        <p className="settings-card-note">
          {t("history.glossary_candidates_manual_hint")}
        </p>
        {candidate.manualError && (
          <p className="history-candidate-error">{candidate.manualError}</p>
        )}
      </div>

      <CandidateActions t={t} candidate={candidate} />
    </div>
  );
}

function CandidateFeedback({
  t,
  entry,
  candidate,
}: {
  t: (key: string) => string;
  entry: HistoryEntry;
  candidate: GlossaryCandidateController;
}) {
  const sendsBoth = entry.rawText !== entry.text && entry.text.trim() !== "";

  return (
    <>
      {candidate.remoteReady && (
        <p className="history-candidate-note">
          {t(
            sendsBoth
              ? "history.glossary_candidates_send_note_both"
              : "history.glossary_candidates_send_note_raw",
          )}
        </p>
      )}

      {candidate.glossaryLoaded &&
        !candidate.remoteReady &&
        !candidate.loading && (
          <p className="history-candidate-hint">
            {t("history.glossary_candidates_remote_missing")}
          </p>
        )}

      {candidate.loading && (
        <p className="history-candidate-loading" role="status">
          <Loader2 size={13} className="history-spin" />
          {t("history.glossary_candidates_loading")}
        </p>
      )}

      {candidate.error && (
        <p className="history-candidate-error" role="alert">
          <AlertCircle size={13} />
          {candidate.error}
        </p>
      )}

      {candidate.remoteReady &&
        !candidate.loading &&
        !candidate.error &&
        candidate.candidates.length === 0 &&
        candidate.addedCount === 0 && (
          <p className="history-candidate-hint">
            {t("history.glossary_candidates_empty")}
          </p>
        )}
    </>
  );
}

function CandidateOptions({
  t,
  candidates,
  selected,
  disabled,
  onToggle,
}: {
  t: (key: string) => string;
  candidates: GlossaryCandidateInfo[];
  selected: Set<string>;
  disabled: boolean;
  onToggle: (term: string) => void;
}) {
  if (candidates.length === 0) return null;

  return (
    <ul className="history-candidate-list">
      {candidates.map((candidate) => (
        <li key={candidate.term}>
          <label className="history-candidate-option">
            <input
              type="checkbox"
              checked={selected.has(candidate.term)}
              disabled={disabled}
              onChange={() => onToggle(candidate.term)}
            />
            <span className="history-candidate-term">{candidate.term}</span>
            {candidate.fragment && (
              <span className="history-candidate-fragment">
                {t("history.glossary_candidates_fragment")}{" "}
                {candidate.fragment}
              </span>
            )}
          </label>
        </li>
      ))}
    </ul>
  );
}

function SkippedTerms({
  t,
  skipped,
}: {
  t: (key: string) => string;
  skipped: SkippedGlossaryTerm[];
}) {
  if (skipped.length === 0) return null;

  return (
    <div className="history-candidate-skipped" role="status">
      <span>{t("history.glossary_candidates_skipped")}</span>
      <ul>
        {skipped.map((item, index) => (
          <li key={`${item.term}:${item.reason}:${index}`}>
            {item.term}
            {t("history.glossary_candidates_skip_separator")}
            {t(`history.glossary_candidates_skip_${item.reason}`)}
          </li>
        ))}
      </ul>
    </div>
  );
}

function CandidateActions({
  t,
  candidate,
}: {
  t: (key: string) => string;
  candidate: GlossaryCandidateController;
}) {
  return (
    <div className="history-candidate-actions">
      <button
        type="button"
        className="btn btn-sm btn-secondary"
        disabled={
          candidate.adding ||
          !candidate.glossaryLoaded ||
          (candidate.selected.size === 0 && candidate.manualTerms.length === 0)
        }
        onClick={candidate.add}
      >
        {candidate.adding && <Loader2 size={13} className="history-spin" />}
        {t("history.glossary_candidates_add")}
      </button>
      <button
        type="button"
        className="btn btn-sm btn-ghost"
        disabled={candidate.adding}
        onClick={candidate.close}
      >
        {t("history.glossary_candidates_close")}
      </button>
      <span className="history-candidate-count">
        {candidate.glossaryCount}/{MAX_GLOSSARY_TERMS}
      </span>
    </div>
  );
}
