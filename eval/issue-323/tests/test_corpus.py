import tempfile
import unittest
from pathlib import Path

from corpus import (
    Sample,
    build_authored_directed,
    build_hard_negatives,
    split_directed,
    split_random,
)


class AuthoredDirectedTest(unittest.TestCase):
    def test_replaces_target_term_with_variant(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "directed.txt"
            path.write_text(
                "地月空间很重要。\n平动点很特殊。\n深空探测周期长。\n",
                encoding="utf-8",
            )
            samples = build_authored_directed(
                path,
                ["地月空间", "平动点", "深空"],
                {
                    "地月空间": ["地约空间"],
                    "平动点": ["平洞点"],
                    "深空": ["神空"],
                },
            )

        self.assertEqual(len(samples), 3)
        self.assertEqual(samples[0].input_text, "地约空间很重要。")
        self.assertEqual(samples[0].expected_terms, ["地月空间"])
        self.assertEqual(samples[0].variant, "地约空间")
        self.assertEqual(samples[1].input_text, "平洞点很特殊。")
        self.assertEqual(samples[2].input_text, "神空探测周期长。")


class HardNegativeTest(unittest.TestCase):
    def test_filters_lines_containing_glossary_terms(self):
        samples = build_hard_negatives(
            ["地月空间很重要。", "地月系统很重要。"], ["地月空间"]
        )

        self.assertEqual(len(samples), 1)
        self.assertEqual(samples[0].input_text, "地月系统很重要。")


class SplitTest(unittest.TestCase):
    def test_directed_split_takes_one_dev_per_term(self):
        samples = [
            Sample("a1", "a", ["A"], "authored"),
            Sample("a2", "a", ["A"], "authored"),
            Sample("a3", "a", ["A"], "authored"),
            Sample("b1", "b", ["B"], "authored"),
            Sample("b2", "b", ["B"], "authored"),
        ]

        dev, heldout = split_directed(samples, dev_per_term=1)

        self.assertEqual(len(dev), 2)
        self.assertEqual(len(heldout), 3)
        self.assertEqual(
            sorted(sample.expected_terms[0] for sample in dev), ["A", "B"]
        )

    def test_random_split_is_deterministic(self):
        samples = [Sample(str(index), "x", [], "history") for index in range(10)]

        first_dev, first_heldout = split_random(samples, ratio=0.5, seed=7)
        second_dev, second_heldout = split_random(samples, ratio=0.5, seed=7)

        self.assertEqual(
            [sample.id for sample in first_dev],
            [sample.id for sample in second_dev],
        )
        self.assertEqual(
            [sample.id for sample in first_heldout],
            [sample.id for sample in second_heldout],
        )
        self.assertEqual(len(first_dev), 5)


if __name__ == "__main__":
    unittest.main()
