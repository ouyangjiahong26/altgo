import { BookOpen } from "lucide-react";
import { useState } from "react";
import { GlossaryChipEditor } from "../../components/GlossaryChipEditor";
import type { AppConfig } from "../../hooks/useConfigForm";
import {
  MAX_GLOSSARY_TERMS,
  glossarySkipMessage,
  mergeGlossaryTerms,
  splitGlossaryInput,
} from "../../utils/glossary";
import { SectionHead } from "./SectionHead";

interface GlossarySectionProps {
  t: (key: string) => string;
  config: AppConfig;
  update: <K extends keyof AppConfig>(key: K, value: AppConfig[K]) => void;
}

/** 术语表：标签式编辑，回车或逗号添加，支持粘贴多行。 */
export function GlossarySection({ t, config, update }: GlossarySectionProps) {
  const [input, setInput] = useState("");
  const [error, setError] = useState("");
  const terms = config.polishGlossary;

  const removeTerm = (term: string) => {
    update(
      "polishGlossary",
      terms.filter((item) => item !== term),
    );
    setError("");
  };

  const addTerms = (rawTerms: string[]): boolean => {
    const result = mergeGlossaryTerms(terms, rawTerms);
    if (result.added.length > 0) {
      update("polishGlossary", result.terms);
    }
    setError(
      result.skipped.length > 0
        ? glossarySkipMessage(result.skipped[0].reason, t)
        : "",
    );
    return result.added.length > 0;
  };

  const addFromInput = (text?: string) => {
    const value = text ?? input;
    if (!value.trim()) return;
    if (addTerms(splitGlossaryInput(value))) {
      setInput("");
    }
  };

  return (
    <section className="settings-section settings-section--glossary">
      <SectionHead
        leading={<BookOpen size={14} aria-hidden="true" />}
        title={t("settings.polish_glossary")}
        note={t("settings.polish_glossary_section_note")}
      />
      <div className="card glossary-card">
        <GlossaryChipEditor
          t={t}
          terms={terms}
          input={input}
          placeholder={t("settings.polish_glossary_add_placeholder")}
          onInputChange={(value) => {
            setInput(value);
            setError("");
          }}
          onAdd={addFromInput}
          onRemove={removeTerm}
        />
        <div className="glossary-meta">
          <p className="settings-card-note">
            {t("settings.polish_glossary_hint")}
          </p>
          <span className="glossary-count">
            {terms.length}/{MAX_GLOSSARY_TERMS}
          </span>
        </div>
        {error && <p className="settings-test-err">{error}</p>}
      </div>
    </section>
  );
}
