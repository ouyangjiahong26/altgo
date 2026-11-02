#!/usr/bin/env python3
"""Issue 323 文本侧评估入口。

读取本机历史与测试术语表，跑基线、拼音替换、精确替换上限、
远端润色与词表润色，输出指标与案例。不修改生产代码。
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
from concurrent.futures import ThreadPoolExecutor
from dataclasses import asdict, dataclass, field
from pathlib import Path

from corpus import (
    Sample,
    build_authored_directed,
    build_hard_negatives,
    build_normal_samples,
    build_real_directed,
    load_history,
    load_lines,
    load_terms,
    load_variant_map,
    split_directed,
    split_random,
    to_dict,
)
from llm import LLMClient, load_config, load_prompt, with_glossary
from metrics import (
    content_diff_ratio,
    entity_fidelity,
    false_replacement_rate,
    homophone_false_replacement_rate,
    term_correction_rate,
)
from replacement import exact_replace, pinyin_replace

ROOT = Path(__file__).resolve().parent
SIZES = (10, 50, 200)


@dataclass
class Settings:
    history_path: Path
    config_path: Path
    prompts_dir: Path
    out_dir: Path
    workers: int
    seed: int
    smoke: bool
    no_llm: bool


@dataclass
class Prepared:
    terms: dict[int, list[str]]
    variant_map: dict[str, list[str]]
    heavy_prompt: str
    directed_dev: list[Sample]
    directed_heldout: list[Sample]
    hard_dev: dict[int, list[Sample]]
    hard_heldout: dict[int, list[Sample]]
    normal_dev: dict[int, list[Sample]]
    normal_heldout: dict[int, list[Sample]]


@dataclass
class MethodSpec:
    name: str
    kind: str
    terms: list[str] = field(default_factory=list)
    system_prompt: str = ""
    glossary_size: int = 10
    variant_map: dict[str, str] = field(default_factory=dict)


@dataclass
class MethodResult:
    output: str
    latency_ms: float
    prompt_tokens: int = 0
    completion_tokens: int = 0
    error: str = ""
    cached: bool = False


def load_settings(args: argparse.Namespace) -> Settings:
    history = Path(args.history).expanduser()
    config = Path(args.config).expanduser()
    prompts = Path(args.prompts).expanduser()
    out = Path(args.out).expanduser()
    for path, label in (
        (history, "history"),
        (config, "config"),
        (prompts / "base.txt", "prompts"),
    ):
        if not path.exists():
            raise SystemExit(f"{label} not found: {path}")
    return Settings(
        history_path=history,
        config_path=config,
        prompts_dir=prompts,
        out_dir=out,
        workers=args.workers,
        seed=args.seed,
        smoke=args.smoke,
        no_llm=args.no_llm,
    )


def prepare(settings: Settings) -> Prepared:
    terms_200 = load_terms(ROOT / "glossaries/terms_200.txt")
    terms = {size: terms_200[:size] for size in SIZES}
    variants = load_variant_map(ROOT / "data/term_variants.txt")
    entries = load_history(settings.history_path)
    heavy_prompt = load_prompt(settings.prompts_dir, "heavy")

    authored = build_authored_directed(
        ROOT / "data/authored_directed.txt", terms[10], variants
    )
    real = build_real_directed(entries, terms[10])
    directed_dev, directed_heldout = split_directed(
        authored + real, dev_per_term=1
    )

    hard_lines = load_lines(ROOT / "data/hard_negatives.txt")
    hard_dev: dict[int, list[Sample]] = {}
    hard_heldout: dict[int, list[Sample]] = {}
    normal_dev: dict[int, list[Sample]] = {}
    normal_heldout: dict[int, list[Sample]] = {}
    for size in SIZES:
        hard = build_hard_negatives(hard_lines, terms[size])
        normal = build_normal_samples(entries, terms[size], count=60, seed=settings.seed)
        hard_dev[size], hard_heldout[size] = split_random(
            hard, ratio=0.5, seed=settings.seed
        )
        normal_dev[size], normal_heldout[size] = split_random(
            normal, ratio=0.5, seed=settings.seed
        )

    return Prepared(
        terms=terms,
        variant_map=variants,
        heavy_prompt=heavy_prompt,
        directed_dev=directed_dev,
        directed_heldout=directed_heldout,
        hard_dev=hard_dev,
        hard_heldout=hard_heldout,
        normal_dev=normal_dev,
        normal_heldout=normal_heldout,
    )


def _baseline_methods(prepared: Prepared) -> list[MethodSpec]:
    return [
        MethodSpec(name="none", kind="identity", glossary_size=10),
        MethodSpec(
            name="pinyin_10",
            kind="pinyin",
            terms=prepared.terms[10],
            glossary_size=10,
        ),
        MethodSpec(
            name="pinyin_50",
            kind="pinyin",
            terms=prepared.terms[50],
            glossary_size=50,
        ),
        MethodSpec(
            name="pinyin_200",
            kind="pinyin",
            terms=prepared.terms[200],
            glossary_size=200,
        ),
        MethodSpec(
            name="exact_10",
            kind="exact",
            variant_map={
                variant: term
                for term, variants in prepared.variant_map.items()
                for variant in variants
            },
            glossary_size=10,
        ),
    ]


def _llm_methods(prepared: Prepared) -> list[MethodSpec]:
    methods = [
        MethodSpec(
            name="heavy_no_glossary",
            kind="llm",
            system_prompt=prepared.heavy_prompt,
            glossary_size=10,
        )
    ]
    for size in SIZES:
        methods.append(
            MethodSpec(
                name=f"heavy_glossary_{size}",
                kind="llm",
                system_prompt=with_glossary(
                    prepared.heavy_prompt, prepared.terms[size]
                ),
                glossary_size=size,
            )
        )
    methods.append(
        MethodSpec(
            name="combo_pinyin_10_heavy",
            kind="combo",
            terms=prepared.terms[10],
            system_prompt=prepared.heavy_prompt,
            glossary_size=10,
        )
    )
    methods.append(
        MethodSpec(
            name="combo_pinyin_10_heavy_glossary_10",
            kind="combo",
            terms=prepared.terms[10],
            system_prompt=with_glossary(
                prepared.heavy_prompt, prepared.terms[10]
            ),
            glossary_size=10,
        )
    )
    return methods


def build_methods(prepared: Prepared, no_llm: bool) -> list[MethodSpec]:
    methods = _baseline_methods(prepared)
    if not no_llm:
        methods.extend(_llm_methods(prepared))
    return methods


def apply_method(
    spec: MethodSpec, sample: Sample, client: LLMClient | None, run: int = 0
) -> MethodResult:
    started = time.perf_counter()
    if spec.kind == "identity":
        return MethodResult(sample.input_text, 0.0)
    if spec.kind == "pinyin":
        output, _ = pinyin_replace(sample.input_text, spec.terms)
        return MethodResult(output, (time.perf_counter() - started) * 1000)
    if spec.kind == "exact":
        output, _ = exact_replace(sample.input_text, spec.variant_map)
        return MethodResult(output, (time.perf_counter() - started) * 1000)
    if client is None:
        raise ValueError(f"method {spec.name} needs an LLM client")

    text = sample.input_text
    pinyin_ms = 0.0
    if spec.kind == "combo":
        pinyin_started = time.perf_counter()
        text, _ = pinyin_replace(text, spec.terms)
        pinyin_ms = (time.perf_counter() - pinyin_started) * 1000
    result = client.polish(spec.system_prompt, text, run=run)
    latency = (
        result.latency_ms + pinyin_ms
        if result.cached
        else (time.perf_counter() - started) * 1000
    )
    return MethodResult(
        output=result.output,
        latency_ms=latency,
        prompt_tokens=result.prompt_tokens,
        completion_tokens=result.completion_tokens,
        error=result.error,
        cached=result.cached,
    )


def run_method(
    spec: MethodSpec,
    samples: list[Sample],
    client: LLMClient | None,
    workers: int,
    run: int = 0,
) -> dict[str, MethodResult]:
    if spec.kind in ("identity", "pinyin", "exact"):
        return {
            sample.id: apply_method(spec, sample, client, run)
            for sample in samples
        }

    def work(sample: Sample) -> tuple[str, MethodResult]:
        return sample.id, apply_method(spec, sample, client, run)

    with ThreadPoolExecutor(max_workers=workers) as pool:
        return dict(pool.map(work, samples))


def percentile(values: list[float], ratio: float) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    index = round((len(ordered) - 1) * ratio)
    return ordered[min(max(index, 0), len(ordered) - 1)]


def _outputs(results: dict[str, MethodResult]) -> dict[str, str]:
    return {sample_id: result.output for sample_id, result in results.items()}


def _latency_stats(results: dict[str, MethodResult]) -> dict:
    values = [
        result.latency_ms
        for result in results.values()
        if result.latency_ms > 0 and not result.error
    ]
    errors = sum(1 for result in results.values() if result.error)
    cached = sum(1 for result in results.values() if result.cached)
    return {
        "p50_ms": round(percentile(values, 0.50), 2),
        "p95_ms": round(percentile(values, 0.95), 2),
        "mean_ms": round(sum(values) / len(values), 2) if values else 0.0,
        "errors": errors,
        "cached": cached,
    }


def _token_stats(results: dict[str, MethodResult], count: int) -> dict:
    prompt = sum(result.prompt_tokens for result in results.values())
    completion = sum(result.completion_tokens for result in results.values())
    return {
        "prompt_tokens": prompt,
        "completion_tokens": completion,
        "prompt_tokens_per_1k": round(prompt / count * 1000, 1) if count else 0.0,
        "completion_tokens_per_1k": round(completion / count * 1000, 1)
        if count
        else 0.0,
    }


def _directed_metrics(
    prepared: Prepared, directed: dict[str, MethodResult]
) -> dict:
    metrics = term_correction_rate(
        [to_dict(sample) for sample in prepared.directed_heldout],
        _outputs(directed),
    )
    return {
        "expected": metrics.expected,
        "correct": metrics.correct,
        "rate": round(metrics.rate, 4),
    }


def _hard_metrics(
    prepared: Prepared, size: int, hard: dict[str, MethodResult]
) -> dict:
    samples = [to_dict(sample) for sample in prepared.hard_heldout[size]]
    metrics = homophone_false_replacement_rate(
        samples, _outputs(hard), prepared.terms[size]
    )
    any_insertion = false_replacement_rate(
        samples, _outputs(hard), prepared.terms[size]
    )
    return {
        "total": metrics.total,
        "false_replacements": metrics.false_replacements,
        "rate": round(metrics.rate, 4),
        "details": [asdict(item) for item in metrics.details],
        "any_insertion": {
            "false_replacements": any_insertion.false_replacements,
            "rate": round(any_insertion.rate, 4),
            "details": [asdict(item) for item in any_insertion.details],
        },
    }


def _normal_metrics(
    prepared: Prepared, size: int, normal: dict[str, MethodResult]
) -> dict:
    entity_total = 0
    entity_kept = 0
    diff_ratios = []
    for sample in prepared.normal_heldout[size]:
        result = normal.get(sample.id)
        if result is None or result.error:
            continue
        fidelity = entity_fidelity(sample.input_text, result.output)
        entity_total += fidelity.total
        entity_kept += fidelity.kept
        diff_ratios.append(
            content_diff_ratio(sample.input_text, result.output)
        )
    return {
        "samples": len(diff_ratios),
        "entity_total": entity_total,
        "entity_kept": entity_kept,
        "entity_rate": round(entity_kept / entity_total, 4)
        if entity_total
        else 1.0,
        "mean_diff_ratio": round(sum(diff_ratios) / len(diff_ratios), 4)
        if diff_ratios
        else 0.0,
        "changed_over_15pct": sum(
            1 for value in diff_ratios if value > 0.15
        ),
    }


def compute_metrics(
    prepared: Prepared,
    spec: MethodSpec,
    directed: dict[str, MethodResult],
    hard: dict[str, MethodResult],
    normal: dict[str, MethodResult],
) -> dict:
    size = spec.glossary_size
    return {
        "directed": _directed_metrics(prepared, directed),
        "hard": _hard_metrics(prepared, size, hard),
        "normal": _normal_metrics(prepared, size, normal),
        "latency": _latency_stats(directed)
        | {"hard": _latency_stats(hard), "normal": _latency_stats(normal)},
        "tokens": _token_stats(directed, len(directed))
        | {
            "hard": _token_stats(hard, len(hard)),
            "normal": _token_stats(normal, len(normal)),
        },
    }


def save_manifest(settings: Settings, prepared: Prepared) -> None:
    manifest = {
        "terms": {str(size): terms for size, terms in prepared.terms.items()},
        "counts": {
            "directed_dev": len(prepared.directed_dev),
            "directed_heldout": len(prepared.directed_heldout),
            "hard": {
                str(size): len(prepared.hard_heldout[size]) for size in SIZES
            },
            "normal": {
                str(size): len(prepared.normal_heldout[size]) for size in SIZES
            },
        },
        "directed_dev": [to_dict(sample) for sample in prepared.directed_dev],
        "directed_heldout": [
            to_dict(sample) for sample in prepared.directed_heldout
        ],
    }
    settings.out_dir.mkdir(parents=True, exist_ok=True)
    (settings.out_dir / "manifest.json").write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2), encoding="utf-8"
    )


def save_outputs(
    settings: Settings,
    prepared: Prepared,
    results: dict[str, dict],
) -> None:
    records = []
    for method_name, entry in results.items():
        size = entry["size"]
        for set_name, samples in (
            ("directed", prepared.directed_heldout),
            ("hard", prepared.hard_heldout[size]),
            ("normal", prepared.normal_heldout[size]),
        ):
            for sample in samples:
                result = entry[set_name].get(sample.id)
                if result is None:
                    continue
                records.append(
                    {
                        "method": method_name,
                        "set": set_name,
                        "sample_id": sample.id,
                        "input_text": sample.input_text,
                        "expected_terms": sample.expected_terms,
                        "output": result.output,
                        "latency_ms": result.latency_ms,
                        "cached": result.cached,
                        "error": result.error,
                    }
                )
    with (settings.out_dir / "outputs.jsonl").open(
        "w", encoding="utf-8"
    ) as handle:
        for record in records:
            handle.write(json.dumps(record, ensure_ascii=False) + "\n")


def _stability_summary(
    samples: list[Sample], runs: list[dict[str, MethodResult]]
) -> dict:
    details = []
    identical = 0
    term_stable = 0
    term_samples = 0
    for sample in samples:
        outputs = [runs[run][sample.id].output for run in range(3)]
        same = len(set(outputs)) == 1
        identical += int(same)
        expected = sample.expected_terms
        if expected:
            term_samples += 1
            if all(
                all(term in output for term in expected) for output in outputs
            ):
                term_stable += 1
        details.append(
            {
                "sample_id": sample.id,
                "expected_terms": expected,
                "identical": same,
                "term_stable": all(
                    all(term in output for term in expected)
                    for output in outputs
                )
                if expected
                else None,
                "outputs": outputs,
            }
        )
    return {
        "samples": len(samples),
        "identical": identical,
        "rate": round(identical / len(samples), 4) if samples else 0.0,
        "term_samples": term_samples,
        "term_stable": term_stable,
        "term_rate": round(term_stable / term_samples, 4)
        if term_samples
        else 0.0,
        "details": details,
    }


def run_stability(
    prepared: Prepared,
    client: LLMClient,
    spec: MethodSpec,
    workers: int,
) -> dict:
    samples = (
        prepared.directed_heldout[:7]
        + prepared.hard_heldout[10][:7]
        + prepared.normal_heldout[10][:6]
    )
    runs = [
        run_method(spec, samples, client, workers, run=run)
        for run in range(3)
    ]
    return _stability_summary(samples, runs)


def save_diffs(
    settings: Settings,
    prepared: Prepared,
    method_name: str,
    normal: dict[str, MethodResult],
    size: int,
) -> None:
    rows = []
    for sample in prepared.normal_heldout[size]:
        result = normal.get(sample.id)
        if result is None or result.error:
            continue
        ratio = content_diff_ratio(sample.input_text, result.output)
        rows.append((ratio, sample, result))
    rows.sort(key=lambda item: item[0], reverse=True)
    lines = [f"# {method_name} 普通句改动排名\n"]
    for ratio, sample, result in rows[:30]:
        lines.append(f"## {sample.id} diff={ratio:.3f}\n")
        lines.append(f"输入：{sample.input_text}\n")
        lines.append(f"输出：{result.output}\n")
    (settings.out_dir / f"normal_diffs_{method_name}.md").write_text(
        "\n".join(lines), encoding="utf-8"
    )


def build_client(settings: Settings) -> LLMClient:
    config = load_config(settings.config_path)
    polisher = config["polisher"]
    level = str(polisher.get("level", "heavy"))
    if level != "heavy":
        raise SystemExit(
            f"evaluation expects heavy polish level, config has {level}"
        )
    api_key = os.environ.get("ALTGO_POLISHER_API_KEY") or polisher.get(
        "api_key", ""
    )
    return LLMClient(
        api_key=api_key,
        base_url=polisher["api_base_url"],
        model=polisher["model"],
        temperature=float(polisher.get("temperature", 0.3)),
        max_tokens=int(polisher.get("max_tokens", 1024)),
        cache_path=settings.out_dir / "cache.jsonl",
    )


def run_methods(
    prepared: Prepared,
    methods: list[MethodSpec],
    client: LLMClient | None,
    settings: Settings,
) -> tuple[dict[str, dict], dict[str, dict]]:
    summary: dict[str, dict] = {}
    all_results: dict[str, dict] = {}
    for spec in methods:
        size = spec.glossary_size
        directed = run_method(
            spec, prepared.directed_heldout, client, settings.workers
        )
        hard = run_method(
            spec, prepared.hard_heldout[size], client, settings.workers
        )
        normal = run_method(
            spec, prepared.normal_heldout[size], client, settings.workers
        )
        all_results[spec.name] = {
            "size": size,
            "directed": directed,
            "hard": hard,
            "normal": normal,
        }
        summary[spec.name] = compute_metrics(
            prepared, spec, directed, hard, normal
        )
        save_diffs(settings, prepared, spec.name, normal, size)
        print(
            f"{spec.name}: directed={summary[spec.name]['directed']['rate']:.3f} "
            f"hard_false={summary[spec.name]['hard']['rate']:.3f} "
            f"entity={summary[spec.name]['normal']['entity_rate']:.3f}",
            flush=True,
        )
    return summary, all_results


def parse_args(argv: list[str] | None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--history", default="~/.config/altgo/history.json")
    parser.add_argument("--config", default="~/.config/altgo/altgo.toml")
    parser.add_argument(
        "--prompts", default=str(ROOT.parent.parent / "resources/prompts")
    )
    parser.add_argument("--out", default=str(ROOT / "results"))
    parser.add_argument("--workers", type=int, default=6)
    parser.add_argument("--seed", type=int, default=323)
    parser.add_argument("--smoke", action="store_true")
    parser.add_argument("--no-llm", action="store_true")
    return parser.parse_args(argv)


def limit_for_smoke(prepared: Prepared) -> None:
    prepared.directed_heldout = prepared.directed_heldout[:2]
    prepared.hard_heldout = {
        size: prepared.hard_heldout[size][:2] for size in SIZES
    }
    prepared.normal_heldout = {
        size: prepared.normal_heldout[size][:2] for size in SIZES
    }


def smoke_methods(methods: list[MethodSpec]) -> list[MethodSpec]:
    names = ("none", "pinyin_10", "heavy_no_glossary", "heavy_glossary_10")
    return [method for method in methods if method.name in names]


def add_stability(
    prepared: Prepared,
    methods: list[MethodSpec],
    client: LLMClient,
    settings: Settings,
    summary: dict[str, dict],
) -> None:
    spec = next(method for method in methods if method.name == "heavy_glossary_10")
    summary["stability_heavy_glossary_10"] = run_stability(
        prepared, client, spec, settings.workers
    )
    print(
        "stability_heavy_glossary_10: "
        f"{summary['stability_heavy_glossary_10']['rate']:.3f}"
    )


def main(argv: list[str] | None = None) -> int:
    settings = load_settings(parse_args(argv))
    if settings.smoke:
        settings.out_dir = settings.out_dir / "smoke"
    settings.out_dir.mkdir(parents=True, exist_ok=True)
    prepared = prepare(settings)
    save_manifest(settings, prepared)

    methods = build_methods(prepared, settings.no_llm)
    if settings.smoke:
        limit_for_smoke(prepared)
        methods = smoke_methods(methods)

    client = None
    if any(method.kind in ("llm", "combo") for method in methods):
        client = build_client(settings)

    summary, all_results = run_methods(prepared, methods, client, settings)
    save_outputs(settings, prepared, all_results)
    if client is not None:
        add_stability(prepared, methods, client, settings, summary)

    (settings.out_dir / "summary.json").write_text(
        json.dumps(summary, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    print(f"wrote {settings.out_dir / 'summary.json'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
