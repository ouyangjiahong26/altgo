import unittest

from metrics import (
    content_diff_ratio,
    entity_fidelity,
    false_replacement_rate,
    homophone_false_replacement_rate,
    term_correction_rate,
)


class TermCorrectionTest(unittest.TestCase):
    def test_counts_each_expected_term(self):
        samples = [
            {"id": "a", "expected_terms": ["地月空间"]},
            {"id": "b", "expected_terms": ["平动点", "深空"]},
        ]
        outputs = {
            "a": "这里讨论地月空间。",
            "b": "平动点位于深空。",
        }

        result = term_correction_rate(samples, outputs)

        self.assertEqual(result.expected, 3)
        self.assertEqual(result.correct, 3)
        self.assertEqual(result.rate, 1.0)

    def test_counts_missing_term(self):
        samples = [{"id": "a", "expected_terms": ["地月空间"]}]
        outputs = {"a": "这里讨论地约空间。"}

        result = term_correction_rate(samples, outputs)

        self.assertEqual(result.expected, 1)
        self.assertEqual(result.correct, 0)
        self.assertEqual(result.rate, 0.0)


class FalseReplacementTest(unittest.TestCase):
    def test_flags_inserted_term(self):
        samples = [{"id": "a", "input_text": "地月系统。"}]
        outputs = {"a": "地月空间。"}

        result = false_replacement_rate(samples, outputs, ["地月空间"])

        self.assertEqual(result.total, 1)
        self.assertEqual(result.false_replacements, 1)
        self.assertEqual(result.rate, 1.0)
        self.assertEqual(result.details[0].sample_id, "a")

    def test_allows_term_already_in_input(self):
        samples = [{"id": "a", "input_text": "地月空间。"}]
        outputs = {"a": "地月空间。"}

        result = false_replacement_rate(samples, outputs, ["地月空间"])

        self.assertEqual(result.false_replacements, 0)


class HomophoneFalseReplacementTest(unittest.TestCase):
    def test_flags_homophone_replacement(self):
        samples = [{"id": "a", "input_text": "双方缔约空间站。"}]
        outputs = {"a": "双方地月空间站。"}

        result = homophone_false_replacement_rate(
            samples, outputs, ["地月空间"]
        )

        self.assertEqual(result.false_replacements, 1)

    def test_ignores_synonym_normalization(self):
        samples = [{"id": "a", "input_text": "地球和月亮。"}]
        outputs = {"a": "地球和月球。"}

        result = homophone_false_replacement_rate(
            samples, outputs, ["月球"]
        )

        self.assertEqual(result.false_replacements, 0)


class EntityFidelityTest(unittest.TestCase):
    def test_keeps_numbers_urls_and_paths(self):
        result = entity_fidelity(
            "访问 https://example.com/a?b=1 ，文件在 /tmp/a.txt ，共 2026 个。",
            "访问 https://example.com/a?b=1，文件在 /tmp/a.txt，共 2026 个。",
        )

        self.assertEqual(result.kept, result.total)
        self.assertEqual(result.rate, 1.0)

    def test_flags_changed_number(self):
        result = entity_fidelity("共 2026 个。", "共 2027 个。")

        self.assertEqual(result.total, 1)
        self.assertEqual(result.kept, 0)
        self.assertEqual(result.rate, 0.0)


class ContentDiffTest(unittest.TestCase):
    def test_ignores_punctuation_and_fillers(self):
        ratio = content_diff_ratio(
            "嗯，这个方案可以。", "这个方案可以。"
        )

        self.assertEqual(ratio, 0.0)

    def test_flags_semantic_change(self):
        ratio = content_diff_ratio(
            "这个方案可以。", "这个方案不行。"
        )

        self.assertGreater(ratio, 0.15)


if __name__ == "__main__":
    unittest.main()
