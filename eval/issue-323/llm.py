"""远端润色调用：prompt 组合、请求构造、重试与结果缓存。"""

from __future__ import annotations

import hashlib
import json
import re
import threading
import time
import tomllib
from dataclasses import dataclass, replace
from pathlib import Path

import requests

THINK_BLOCK = re.compile(r"<think[^>]*>.*?</think>", re.DOTALL | re.IGNORECASE)
TEMPLATE_LEAK = re.compile(r"<\|[^|]*\|>|</?s>", re.IGNORECASE)
ASSISTANT_PREFIX = re.compile(r"^\s*assistant\s*:", re.IGNORECASE)

GLOSSARY_HEADER = (
    "\n\n用户指定术语表（以下词是标准写法。只有当原文中的词与术语表同音或近音，"
    "且上下文支持时，才改成术语表写法。不要为了使用术语表而插入新词。"
    "不要改动术语表以外的内容）：\n"
)


def load_config(path: Path) -> dict:
    return tomllib.loads(path.read_text(encoding="utf-8"))


def load_prompt(prompts_dir: Path, level: str) -> str:
    base = (prompts_dir / "base.txt").read_text(encoding="utf-8").strip()
    suffix = (prompts_dir / f"{level}-suffix.txt").read_text(
        encoding="utf-8"
    ).strip()
    return f"{base}\n\n{suffix}"


def with_glossary(system_prompt: str, terms: list[str]) -> str:
    return system_prompt + GLOSSARY_HEADER + "\n".join(terms)


def endpoint_url(base_url: str) -> str:
    trimmed = base_url.strip().rstrip("/")
    if not trimmed:
        raise ValueError("empty base URL")
    scheme_end = trimmed.find("://")
    path_start = (
        trimmed.find("/", scheme_end + 3)
        if scheme_end != -1
        else trimmed.find("/")
    )
    path = trimmed[path_start:] if path_start != -1 else ""
    request_path = "/chat/completions"
    if path.endswith(request_path):
        return trimmed
    if not path:
        return f"{trimmed}/v1{request_path}"
    return f"{trimmed}{request_path}"


def host_of(url: str) -> str:
    authority = url.split("://", 1)[-1].split("/", 1)[0]
    return authority.split(":", 1)[0].lower()


def host_matches(host: str, domain: str) -> bool:
    return host == domain or host.endswith(f".{domain}")


def build_body(
    model: str,
    system_prompt: str,
    text: str,
    host: str,
    temperature: float = 0.3,
    max_tokens: int = 1024,
) -> dict:
    body = {
        "model": model,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": text},
        ],
        "temperature": temperature,
        "max_tokens": max_tokens,
    }
    if host_matches(host, "deepseek.com"):
        body["thinking"] = {"type": "disabled"}
    return body


def strip_thinking_tags(text: str) -> str:
    text = THINK_BLOCK.sub("", text)
    text = TEMPLATE_LEAK.sub("", text)
    text = ASSISTANT_PREFIX.sub("", text)
    return text.strip()


@dataclass
class LLMResult:
    output: str
    prompt_tokens: int
    completion_tokens: int
    latency_ms: float
    error: str = ""
    cached: bool = False


class LLMClient:
    def __init__(
        self,
        api_key: str,
        base_url: str,
        model: str,
        temperature: float,
        max_tokens: int,
        cache_path: Path | None = None,
        timeout: float = 60.0,
        max_attempts: int = 3,
    ) -> None:
        if not api_key:
            raise ValueError("missing API key")
        self.api_key = api_key
        self.model = model
        self.temperature = temperature
        self.max_tokens = max_tokens
        self.timeout = timeout
        self.max_attempts = max_attempts
        self.endpoint = endpoint_url(base_url)
        self.host = host_of(base_url)
        self.cache_path = cache_path
        self._cache: dict[str, LLMResult] = {}
        self._lock = threading.Lock()
        if cache_path is not None and cache_path.exists():
            self._load_cache()

    def _load_cache(self) -> None:
        assert self.cache_path is not None
        for line in self.cache_path.read_text(encoding="utf-8").splitlines():
            if not line.strip():
                continue
            try:
                item = json.loads(line)
                self._cache[item["key"]] = LLMResult(
                    output=item["output"],
                    prompt_tokens=item["prompt_tokens"],
                    completion_tokens=item["completion_tokens"],
                    latency_ms=item["latency_ms"],
                    error=item.get("error", ""),
                    cached=True,
                )
            except (KeyError, json.JSONDecodeError):
                continue

    def _cache_key(self, system_prompt: str, text: str, run: int) -> str:
        payload = f"{self.model}\n{system_prompt}\n{text}\n{run}"
        return hashlib.sha256(payload.encode("utf-8")).hexdigest()

    def _append_cache(self, key: str, result: LLMResult) -> None:
        if self.cache_path is None or result.error:
            return
        with self._lock:
            self.cache_path.parent.mkdir(parents=True, exist_ok=True)
            with self.cache_path.open("a", encoding="utf-8") as handle:
                handle.write(
                    json.dumps(
                        {
                            "key": key,
                            "output": result.output,
                            "prompt_tokens": result.prompt_tokens,
                            "completion_tokens": result.completion_tokens,
                            "latency_ms": result.latency_ms,
                        },
                        ensure_ascii=False,
                    )
                    + "\n"
                )

    def polish(
        self, system_prompt: str, text: str, run: int = 0
    ) -> LLMResult:
        key = self._cache_key(system_prompt, text, run)
        with self._lock:
            cached = self._cache.get(key)
        if cached is not None and not cached.error:
            return replace(cached, cached=True)

        body = build_body(
            self.model,
            system_prompt,
            text,
            self.host,
            self.temperature,
            self.max_tokens,
        )
        started = time.perf_counter()
        result = self._request(body)
        result.latency_ms = (time.perf_counter() - started) * 1000
        if not result.error:
            with self._lock:
                self._cache[key] = result
            self._append_cache(key, result)
        return result

    def _request(self, body: dict) -> LLMResult:
        last_error = ""
        for attempt in range(self.max_attempts):
            try:
                response = requests.post(
                    self.endpoint,
                    headers={
                        "Authorization": f"Bearer {self.api_key}",
                        "Content-Type": "application/json",
                    },
                    json=body,
                    timeout=self.timeout,
                )
            except requests.RequestException as error:
                last_error = f"network: {error}"
                if attempt + 1 < self.max_attempts:
                    time.sleep(2**attempt)
                    continue
                return LLMResult("", 0, 0, 0.0, last_error)

            if response.status_code == 200:
                return self._parse(response)
            if response.status_code in (400, 401, 403, 404):
                return LLMResult(
                    "", 0, 0, 0.0, f"http {response.status_code}: {response.text[:300]}"
                )
            last_error = f"http {response.status_code}: {response.text[:300]}"
            if attempt + 1 < self.max_attempts:
                time.sleep(2**attempt)
        return LLMResult("", 0, 0, 0.0, last_error)

    def _parse(self, response: requests.Response) -> LLMResult:
        try:
            data = response.json()
        except ValueError:
            return LLMResult("", 0, 0, 0.0, "invalid JSON response")
        choices = data.get("choices") or []
        if not choices:
            return LLMResult("", 0, 0, 0.0, "empty choices")
        content = choices[0].get("message", {}).get("content") or ""
        content = strip_thinking_tags(content)
        usage = data.get("usage") or {}
        result = LLMResult(
            output=content,
            prompt_tokens=int(usage.get("prompt_tokens") or 0),
            completion_tokens=int(usage.get("completion_tokens") or 0),
            latency_ms=0.0,
        )
        if not content:
            result.error = "empty content"
        return result
