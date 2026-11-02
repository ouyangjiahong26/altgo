/** 术语表的共享规则：合并、去重、长度与条数上限。设置页与历史候选面板
 *  共用同一套规则，避免两处各自判断导致上限不一致。 */

export const MAX_GLOSSARY_TERMS = 50;
export const MAX_GLOSSARY_TERM_CHARS = 32;
export const GLOSSARY_SEPARATORS = /[\n,，]+/;

export type GlossarySkipReason = "duplicate" | "too_long" | "max";

export interface SkippedGlossaryTerm {
  term: string;
  reason: GlossarySkipReason;
}

export interface GlossaryMergeResult {
  terms: string[];
  added: string[];
  skipped: SkippedGlossaryTerm[];
}

/** 未加入原因的提示文案。设置页与历史候选面板共用，避免两套说法。 */
export function glossarySkipMessage(
  reason: SkippedGlossaryTerm["reason"],
  t: (key: string) => string,
): string {
  switch (reason) {
    case "duplicate":
      return t("settings.polish_glossary_duplicate");
    case "too_long":
      return t("settings.polish_glossary_too_long");
    case "max":
      return t("settings.polish_glossary_max");
  }
}

/** 按顺序把新词并入现有术语表。超长、重复与超出条数上限的词不写入，
 *  在 `skipped` 里保留原因，由调用方决定怎么提示。 */
export function mergeGlossaryTerms(
  existing: string[],
  incoming: string[],
): GlossaryMergeResult {
  const terms = existing.map((term) => term.trim()).filter(Boolean);
  const seen = new Set(terms);
  const added: string[] = [];
  const skipped: SkippedGlossaryTerm[] = [];

  for (const raw of incoming) {
    const term = raw.trim();
    if (!term) continue;
    if (Array.from(term).length > MAX_GLOSSARY_TERM_CHARS) {
      skipped.push({ term, reason: "too_long" });
      continue;
    }
    if (seen.has(term)) {
      skipped.push({ term, reason: "duplicate" });
      continue;
    }
    if (terms.length >= MAX_GLOSSARY_TERMS) {
      skipped.push({ term, reason: "max" });
      continue;
    }
    terms.push(term);
    seen.add(term);
    added.push(term);
  }

  return { terms, added, skipped };
}

/** 把输入框或粘贴内容拆成术语。支持换行、半角逗号与全角逗号。 */
export function splitGlossaryInput(text: string): string[] {
  return text
    .split(GLOSSARY_SEPARATORS)
    .map((term) => term.trim())
    .filter(Boolean);
}

/** 条目里是否含汉字。候选筛选第一版只处理中文文本。 */
export function hasCjk(text: string): boolean {
  return /[\u4e00-\u9fff]/.test(text);
}
