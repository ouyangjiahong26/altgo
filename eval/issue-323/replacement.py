"""文本侧术语替换：拼音匹配与变体表匹配。

只处理中文术语。拼音匹配从标准写法生成读音序列，在中文片段上滑窗，
把同音片段替换成标准写法。变体表匹配用于手工补变体的上限对照。
"""

from __future__ import annotations

import re
from dataclasses import dataclass

from pypinyin import Style, lazy_pinyin

HAN_RUN = re.compile(r"[\u4e00-\u9fff]+")


@dataclass(frozen=True)
class Replacement:
    start: int
    end: int
    original: str
    term: str


def pinyin_sequence(text: str) -> list[str]:
    """返回与输入字符大致对齐的无调拼音序列，非中文片段原样保留。"""
    return [
        item.strip()
        for item in lazy_pinyin(text, style=Style.NORMAL, errors="default")
    ]


def _is_chinese_term(term: str) -> bool:
    return len(term) >= 2 and all("\u4e00" <= char <= "\u9fff" for char in term)


def _select_non_overlapping(
    candidates: list[Replacement],
) -> list[Replacement]:
    """长匹配优先，长度相同时起点靠前优先。"""
    selected: list[Replacement] = []
    for candidate in sorted(
        candidates, key=lambda item: (-(item.end - item.start), item.start)
    ):
        if any(
            candidate.start < chosen.end and candidate.end > chosen.start
            for chosen in selected
        ):
            continue
        selected.append(candidate)
    return sorted(selected, key=lambda item: item.start)


def _apply(text: str, replacements: list[Replacement]) -> str:
    output = text
    for item in sorted(replacements, key=lambda value: value.start, reverse=True):
        output = output[: item.start] + item.term + output[item.end :]
    return output


def pinyin_replace(text: str, terms: list[str]) -> tuple[str, list[Replacement]]:
    """把与标准写法同音的中文片段替换成标准写法。"""
    chinese_terms = [term for term in terms if _is_chinese_term(term)]
    if not chinese_terms:
        return text, []

    term_pinyin: dict[tuple[str, ...], str] = {}
    widths: set[int] = set()
    for term in chinese_terms:
        pronunciation = tuple(pinyin_sequence(term))
        term_pinyin.setdefault(pronunciation, term)
        widths.add(len(term))

    term_set = set(chinese_terms)
    candidates: list[Replacement] = []

    for run_match in HAN_RUN.finditer(text):
        run = run_match.group(0)
        pronunciation = pinyin_sequence(run)
        if len(pronunciation) != len(run):
            continue
        for width in sorted(widths):
            if width > len(run):
                continue
            for offset in range(len(run) - width + 1):
                window = run[offset : offset + width]
                if window in term_set:
                    continue
                term = term_pinyin.get(
                    tuple(pronunciation[offset : offset + width])
                )
                if term is None or window == term:
                    continue
                start = run_match.start() + offset
                candidates.append(
                    Replacement(
                        start=start,
                        end=start + width,
                        original=window,
                        term=term,
                    )
                )

    selected = _select_non_overlapping(candidates)
    return _apply(text, selected), selected


def has_pinyin_match(text: str, term: str) -> bool:
    """判断文本里是否存在与术语同音的等长中文片段。"""
    if not _is_chinese_term(term):
        return False
    target = tuple(pinyin_sequence(term))
    width = len(term)
    for run_match in HAN_RUN.finditer(text):
        run = run_match.group(0)
        pronunciation = pinyin_sequence(run)
        if len(pronunciation) != len(run):
            continue
        for offset in range(len(run) - width + 1):
            if tuple(pronunciation[offset : offset + width]) == target:
                return True
    return False


def exact_replace(
    text: str, variant_map: dict[str, str]
) -> tuple[str, list[Replacement]]:
    """按变体表做最长匹配替换。"""
    candidates: list[Replacement] = []
    for variant, term in sorted(
        variant_map.items(), key=lambda item: len(item[0]), reverse=True
    ):
        if not variant:
            continue
        start = text.find(variant)
        while start != -1:
            candidates.append(
                Replacement(
                    start=start,
                    end=start + len(variant),
                    original=variant,
                    term=term,
                )
            )
            start = text.find(variant, start + len(variant))

    selected = _select_non_overlapping(candidates)
    return _apply(text, selected), selected
