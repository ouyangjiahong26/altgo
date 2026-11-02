import tempfile
import unittest
from pathlib import Path

from llm import (
    build_body,
    endpoint_url,
    load_prompt,
    strip_thinking_tags,
    with_glossary,
)


class PromptTest(unittest.TestCase):
    def test_load_prompt_combines_base_and_suffix(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp)
            (path / "base.txt").write_text("base prompt\n", encoding="utf-8")
            (path / "heavy-suffix.txt").write_text(
                "heavy suffix\n", encoding="utf-8"
            )

            prompt = load_prompt(path, "heavy")

        self.assertEqual(prompt, "base prompt\n\nheavy suffix")

    def test_with_glossary_appends_terms_and_instruction(self):
        prompt = with_glossary("base", ["地月空间", "平动点"])

        self.assertTrue(prompt.startswith("base"))
        self.assertIn("用户指定术语表", prompt)
        self.assertIn("地月空间", prompt)
        self.assertIn("平动点", prompt)
        self.assertIn("不要为了使用术语表而插入新词", prompt)


class EndpointTest(unittest.TestCase):
    def test_adds_v1_for_origin_base(self):
        self.assertEqual(
            endpoint_url("https://api.deepseek.com/"),
            "https://api.deepseek.com/v1/chat/completions",
        )

    def test_keeps_existing_version_path(self):
        self.assertEqual(
            endpoint_url("https://api.moonshot.cn/v1"),
            "https://api.moonshot.cn/v1/chat/completions",
        )


class BodyTest(unittest.TestCase):
    def test_disables_thinking_for_deepseek(self):
        body = build_body(
            "deepseek-flash",
            "system",
            "user text",
            "api.deepseek.com",
        )

        self.assertEqual(body["thinking"], {"type": "disabled"})
        self.assertEqual(body["model"], "deepseek-flash")
        self.assertEqual(body["messages"][1]["content"], "user text")

    def test_omits_thinking_for_unknown_host(self):
        body = build_body("model", "system", "user", "api.openai.com")

        self.assertNotIn("thinking", body)


class CleanupTest(unittest.TestCase):
    def test_strips_thinking_block(self):
        self.assertEqual(
            strip_thinking_tags("<think>hidden</think>可见文本"),
            "可见文本",
        )


if __name__ == "__main__":
    unittest.main()
