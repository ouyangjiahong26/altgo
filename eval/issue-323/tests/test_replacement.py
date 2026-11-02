import unittest

from replacement import (
    exact_replace,
    has_pinyin_match,
    pinyin_replace,
    pinyin_sequence,
)


class PinyinReplaceTest(unittest.TestCase):
    def test_replaces_exact_homophone(self):
        output, replacements = pinyin_replace("这里讨论地约空间动力学。", ["地月空间"])

        self.assertEqual(output, "这里讨论地月空间动力学。")
        self.assertEqual(len(replacements), 1)
        self.assertEqual(replacements[0].original, "地约空间")
        self.assertEqual(replacements[0].term, "地月空间")

    def test_replaces_second_homophone(self):
        output, replacements = pinyin_replace("地阅空间相关文献很多。", ["地月空间"])

        self.assertEqual(output, "地月空间相关文献很多。")
        self.assertEqual(replacements[0].original, "地阅空间")

    def test_leaves_non_homophone_untouched(self):
        output, replacements = pinyin_replace("地月系统与地月转移轨道。", ["地月空间"])

        self.assertEqual(output, "地月系统与地月转移轨道。")
        self.assertEqual(replacements, [])

    def test_leaves_near_homophone_untouched(self):
        output, replacements = pinyin_replace("近月空间和月球背面。", ["地月空间"])

        self.assertEqual(output, "近月空间和月球背面。")
        self.assertEqual(replacements, [])

    def test_does_not_replace_standard_term(self):
        output, replacements = pinyin_replace("地月空间是研究重点。", ["地月空间"])

        self.assertEqual(output, "地月空间是研究重点。")
        self.assertEqual(replacements, [])

    def test_longest_term_wins(self):
        output, replacements = pinyin_replace(
            "地约空间站正在建设。", ["地月空间", "地月空间站"]
        )

        self.assertEqual(output, "地月空间站正在建设。")
        self.assertEqual(replacements[0].term, "地月空间站")

    def test_is_idempotent(self):
        first, _ = pinyin_replace("地约空间。", ["地月空间"])
        second, replacements = pinyin_replace(first, ["地月空间"])

        self.assertEqual(first, second)
        self.assertEqual(replacements, [])

    def test_ignores_single_character_terms(self):
        output, replacements = pinyin_replace("这是一个测试。", ["测"])

        self.assertEqual(output, "这是一个测试。")
        self.assertEqual(replacements, [])

    def test_ignores_english_terms(self):
        output, replacements = pinyin_replace("NRHO 与 Halo 轨道。", ["NRHO", "Halo"])

        self.assertEqual(output, "NRHO 与 Halo 轨道。")
        self.assertEqual(replacements, [])

    def test_pinyin_sequence_handles_mixed_text(self):
        self.assertEqual(
            pinyin_sequence("地月空间 NRHO"), ["di", "yue", "kong", "jian", "NRHO"]
        )


class PinyinMatchTest(unittest.TestCase):
    def test_detects_homophone_window(self):
        self.assertTrue(has_pinyin_match("双方缔约空间站。", "地月空间"))

    def test_rejects_synonym_without_homophone(self):
        self.assertFalse(has_pinyin_match("地月系统包括地球和月亮。", "月球"))

    def test_rejects_longer_term(self):
        self.assertFalse(has_pinyin_match("地月转移轨道。", "地月空间"))


class ExactReplaceTest(unittest.TestCase):
    def test_replaces_known_variant(self):
        output, replacements = exact_replace(
            "地约空间动力学。", {"地约空间": "地月空间"}
        )

        self.assertEqual(output, "地月空间动力学。")
        self.assertEqual(replacements[0].term, "地月空间")

    def test_longest_variant_wins(self):
        output, _ = exact_replace(
            "缔约空间站。", {"缔约空间": "地月空间", "缔约空间站": "地月空间站"}
        )

        self.assertEqual(output, "地月空间站。")

    def test_leaves_hard_negative_untouched(self):
        output, replacements = exact_replace("地月系统。", {"地约空间": "地月空间"})

        self.assertEqual(output, "地月系统。")
        self.assertEqual(replacements, [])


if __name__ == "__main__":
    unittest.main()
