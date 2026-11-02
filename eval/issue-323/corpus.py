"""构建文本侧评估语料：定向句、硬负例与普通句。"""

from __future__ import annotations

import json
import random
from dataclasses import asdict, dataclass
from pathlib import Path

from replacement import pinyin_replace


@dataclass(frozen=True)
class Sample:
    id: str
    input_text: str
    expected_terms: list[str]
    source: str
    variant: str = ""
    reference: str = ""


def load_terms(path: Path, limit: int | None = None) -> list[str]:
    terms = [
        line.strip()
        for line in path.read_text(encoding="utf-8").splitlines()
        if line.strip() and not line.strip().startswith("#")
    ]
    if limit is not None:
        terms = terms[:limit]
    return terms


def load_lines(path: Path) -> list[str]:
    return [
        line.strip()
        for line in path.read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]


def load_variant_map(path: Path) -> dict[str, list[str]]:
    mapping: dict[str, list[str]] = {}
    for line in load_lines(path):
        term, _, variants = line.partition(":")
        mapping[term.strip()] = [
            variant.strip() for variant in variants.split(",") if variant.strip()
        ]
    return mapping


def load_history(path: Path) -> list[dict]:
    data = json.loads(path.read_text(encoding="utf-8"))
    return data.get("entries", [])


def _pick_term(text: str, terms: list[str]) -> str | None:
    matches = [term for term in terms if term in text]
    if not matches:
        return None
    return max(matches, key=len)


def build_authored_directed(
    path: Path, terms: list[str], variants: dict[str, list[str]]
) -> list[Sample]:
    samples: list[Sample] = []
    for line in load_lines(path):
        term = _pick_term(line, terms)
        if term is None:
            continue
        term_variants = variants.get(term, [])
        if not term_variants:
            continue
        variant = term_variants[0]
        samples.append(
            Sample(
                id=f"dir_authored_{len(samples):03d}",
                input_text=line.replace(term, variant),
                expected_terms=[term],
                source="authored",
                variant=variant,
                reference=line,
            )
        )
    return samples


def build_real_directed(entries: list[dict], terms: list[str]) -> list[Sample]:
    samples: list[Sample] = []
    seen: set[str] = set()
    for entry in entries:
        raw_text = (entry.get("rawText") or "").strip()
        if not raw_text or raw_text in seen:
            continue
        _, replacements = pinyin_replace(raw_text, terms)
        if not replacements:
            continue
        seen.add(raw_text)
        expected = sorted({replacement.term for replacement in replacements})
        samples.append(
            Sample(
                id=f"dir_real_{len(samples):03d}",
                input_text=raw_text,
                expected_terms=expected,
                source="history_real",
                variant=",".join(
                    sorted({replacement.original for replacement in replacements})
                ),
            )
        )
    return samples


def build_hard_negatives(lines: list[str], terms: list[str]) -> list[Sample]:
    samples: list[Sample] = []
    for line in lines:
        if any(term in line for term in terms):
            continue
        samples.append(
            Sample(
                id=f"hard_{len(samples):03d}",
                input_text=line,
                expected_terms=[],
                source="authored",
            )
        )
    return samples


def build_normal_samples(
    entries: list[dict], terms: list[str], count: int, seed: int
) -> list[Sample]:
    shuffled = list(entries)
    random.Random(seed).shuffle(shuffled)
    samples: list[Sample] = []
    seen: set[str] = set()
    for entry in shuffled:
        text = (entry.get("rawText") or "").strip()
        if not text or text in seen:
            continue
        if not 10 <= len(text) <= 300:
            continue
        if any(term in text for term in terms):
            continue
        _, replacements = pinyin_replace(text, terms)
        if replacements:
            continue
        seen.add(text)
        samples.append(
            Sample(
                id=f"norm_{len(samples):03d}",
                input_text=text,
                expected_terms=[],
                source="history",
            )
        )
        if len(samples) >= count:
            break
    return samples


def split_directed(
    samples: list[Sample], dev_per_term: int = 1
) -> tuple[list[Sample], list[Sample]]:
    groups: dict[str, list[Sample]] = {}
    for sample in samples:
        key = sample.expected_terms[0] if sample.expected_terms else "none"
        groups.setdefault(key, []).append(sample)

    dev: list[Sample] = []
    heldout: list[Sample] = []
    for key in sorted(groups):
        ordered = sorted(groups[key], key=lambda sample: sample.id)
        dev.extend(ordered[:dev_per_term])
        heldout.extend(ordered[dev_per_term:])
    return dev, heldout


def split_random(
    samples: list[Sample], ratio: float, seed: int
) -> tuple[list[Sample], list[Sample]]:
    shuffled = list(samples)
    random.Random(seed).shuffle(shuffled)
    cut = round(len(shuffled) * ratio)
    return shuffled[:cut], shuffled[cut:]


def to_dict(sample: Sample) -> dict:
    return asdict(sample)
