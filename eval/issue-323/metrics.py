"""评估指标：术语修正、误替换与实体保真。"""

from __future__ import annotations

import difflib
import re
from dataclasses import dataclass, field

from replacement import has_pinyin_match

NUMBERS = re.compile(r"\d+(?:[.:]\d+)*")
URLS = re.compile(r"https?://[^\s，。；、）)】]+")
PATHS = re.compile(r"[A-Za-z0-9_.\-]*[\\/][A-Za-z0-9_.\-/\\]*[A-Za-z0-9_]")
PUNCTUATION = re.compile(
    r"[\s，。！？；：、,.!?;:()（）\[\]【】“”\"'‘’]"
)
FILLERS = re.compile(r"(嗯|呃|啊|哦|额|哈哈|对吧|然后)")


@dataclass
class TermCorrectionDetail:
    sample_id: str
    term: str
    hit: bool


@dataclass
class FalseReplacementDetail:
    sample_id: str
    inserted_terms: list[str]


@dataclass
class EntityFidelityDetail:
    entity: str
    kept: bool


@dataclass
class TermCorrectionResult:
    expected: int
    correct: int
    rate: float
    details: list[TermCorrectionDetail] = field(default_factory=list)


@dataclass
class FalseReplacementResult:
    total: int
    false_replacements: int
    rate: float
    details: list[FalseReplacementDetail] = field(default_factory=list)


@dataclass
class EntityFidelityResult:
    total: int
    kept: int
    rate: float
    details: list[EntityFidelityDetail] = field(default_factory=list)


def term_correction_rate(
    samples: list[dict], outputs: dict[str, str]
) -> TermCorrectionResult:
    """按期望术语逐项统计输出里是否出现标准写法。"""
    expected = 0
    correct = 0
    details: list[TermCorrectionDetail] = []
    for sample in samples:
        output = outputs.get(sample["id"], "")
        for term in sample.get("expected_terms", []):
            expected += 1
            hit = term in output
            correct += int(hit)
            details.append(
                TermCorrectionDetail(
                    sample_id=sample["id"],
                    term=term,
                    hit=hit,
                )
            )
    rate = correct / expected if expected else 0.0
    return TermCorrectionResult(expected, correct, rate, details)


def false_replacement_rate(
    samples: list[dict], outputs: dict[str, str], terms: list[str]
) -> FalseReplacementResult:
    """统计硬负例里被凭空插入或替换成术语表的样本数。

    调用方应传入输入不含目标术语的样本。同一句里插入多个术语只计一次。
    """
    total = 0
    false_replacements = 0
    details: list[FalseReplacementDetail] = []
    for sample in samples:
        total += 1
        input_text = sample.get("input_text", "")
        output = outputs.get(sample["id"], "")
        inserted = [
            term
            for term in terms
            if term in output and term not in input_text
        ]
        if inserted:
            false_replacements += 1
            details.append(
                FalseReplacementDetail(
                    sample_id=sample["id"],
                    inserted_terms=inserted,
                )
            )
    rate = false_replacements / total if total else 0.0
    return FalseReplacementResult(total, false_replacements, rate, details)


def homophone_false_replacement_rate(
    samples: list[dict], outputs: dict[str, str], terms: list[str]
) -> FalseReplacementResult:
    """统计硬负例里被同音片段误导、替换成术语的样本数。

    只看输入里存在与术语同音的等长片段的情况。大模型把同义词改成术语
    （如把月亮改成月球）不算同音误替换，另计入任何术语插入统计。
    """
    total = 0
    false_replacements = 0
    details: list[FalseReplacementDetail] = []
    for sample in samples:
        total += 1
        input_text = sample.get("input_text", "")
        output = outputs.get(sample["id"], "")
        inserted = [
            term
            for term in terms
            if term in output
            and term not in input_text
            and has_pinyin_match(input_text, term)
        ]
        if inserted:
            false_replacements += 1
            details.append(
                FalseReplacementDetail(
                    sample_id=sample["id"],
                    inserted_terms=inserted,
                )
            )
    rate = false_replacements / total if total else 0.0
    return FalseReplacementResult(total, false_replacements, rate, details)


def _boundary_free(text: str, start: int, end: int) -> bool:
    before = text[start - 1] if start > 0 else ""
    after = text[end] if end < len(text) else ""
    return not (before.isascii() and before.isalnum()) and not (
        after.isascii() and after.isalnum()
    )


def _contains_entity(text: str, entity: str) -> bool:
    start = text.find(entity)
    while start != -1:
        end = start + len(entity)
        if _boundary_free(text, start, end):
            return True
        start = text.find(entity, start + len(entity))
    return False


def _entities(text: str) -> list[str]:
    found: list[str] = []
    for pattern in (URLS, PATHS, NUMBERS):
        for match in pattern.finditer(text):
            value = match.group(0)
            if value and value not in found:
                found.append(value)
    return found


def entity_fidelity(input_text: str, output_text: str) -> EntityFidelityResult:
    """检查数字、URL 与路径是否逐字保留。"""
    entities = _entities(input_text)
    kept = 0
    details: list[EntityFidelityDetail] = []
    for entity in entities:
        hit = _contains_entity(output_text, entity)
        kept += int(hit)
        details.append(EntityFidelityDetail(entity=entity, kept=hit))
    rate = kept / len(entities) if entities else 1.0
    return EntityFidelityResult(len(entities), kept, rate, details)


def content_diff_ratio(input_text: str, output_text: str) -> float:
    """去掉标点与常见口头语后的字面差异比例。"""
    def normalize(text: str) -> str:
        text = PUNCTUATION.sub("", text)
        text = FILLERS.sub("", text)
        return text

    left = normalize(input_text)
    right = normalize(output_text)
    if not left and not right:
        return 0.0
    if not left or not right:
        return 1.0
    return 1.0 - difflib.SequenceMatcher(None, left, right).ratio()
