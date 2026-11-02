import { BookOpen } from "lucide-react";
import { useRef, useState } from "react";
import type { AppConfig } from "../../hooks/useConfigForm";
import { SectionHead } from "./SectionHead";

const MAX_TERMS = 50;
const MAX_TERM_CHARS = 32;
const SEPARATORS = /[\n,，]+/;

interface GlossarySectionProps {
  t: (key: string) => string;
  config: AppConfig;
  update: <K extends keyof AppConfig>(key: K, value: AppConfig[K]) => void;
}

/** 术语表：标签式编辑，回车或逗号添加，支持粘贴多行。 */
export function GlossarySection({ t, config, update }: GlossarySectionProps) {
  const [input, setInput] = useState("");
  const [error, setError] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  const terms = config.polishGlossary;

  const removeTerm = (term: string) => {
    update(
      "polishGlossary",
      terms.filter((item) => item !== term),
    );
    setError("");
  };

  const addTerms = (rawTerms: string[]): boolean => {
    const next = [...terms];
    const seen = new Set(next);
    let firstError = "";
    for (const raw of rawTerms) {
      const term = raw.trim();
      if (!term) continue;
      if (term.length > MAX_TERM_CHARS) {
        firstError ||= t("settings.polish_glossary_too_long");
        continue;
      }
      if (seen.has(term)) {
        firstError ||= t("settings.polish_glossary_duplicate");
        continue;
      }
      if (next.length >= MAX_TERMS) {
        firstError ||= t("settings.polish_glossary_max");
        break;
      }
      next.push(term);
      seen.add(term);
    }
    const added = next.length !== terms.length;
    if (added) {
      update("polishGlossary", next);
    }
    setError(firstError);
    return added;
  };

  const addFromInput = () => {
    if (!input.trim()) return;
    if (addTerms([input])) {
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
        <div
          className="glossary-editor"
          onClick={() => inputRef.current?.focus()}
        >
          {terms.map((term) => (
            <span className="glossary-chip" key={term}>
              <span>{term}</span>
              <button
                type="button"
                className="glossary-chip-remove"
                aria-label={`${t("settings.polish_glossary_remove")} ${term}`}
                onClick={() => removeTerm(term)}
              >
                ×
              </button>
            </span>
          ))}
          <input
            ref={inputRef}
            className="glossary-input"
            type="text"
            value={input}
            placeholder={t("settings.polish_glossary_add_placeholder")}
            aria-label={t("settings.polish_glossary_add_placeholder")}
            onChange={(e) => {
              setInput(e.target.value);
              setError("");
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === "," || e.key === "，") {
                e.preventDefault();
                addFromInput();
              } else if (e.key === "Backspace" && !input && terms.length > 0) {
                removeTerm(terms[terms.length - 1]);
              }
            }}
            onPaste={(e) => {
              const text = e.clipboardData.getData("text");
              if (!SEPARATORS.test(text)) return;
              e.preventDefault();
              addTerms(text.split(SEPARATORS));
            }}
          />
        </div>
        <div className="glossary-meta">
          <p className="settings-card-note">
            {t("settings.polish_glossary_hint")}
          </p>
          <span className="glossary-count">
            {terms.length}/{MAX_TERMS}
          </span>
        </div>
        {error && <p className="settings-test-err">{error}</p>}
      </div>
    </section>
  );
}
