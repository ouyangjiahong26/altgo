//! 用户自设术语表：prompt 追加块与输出后置检查。

use pinyin::ToPinyin;

const CJK_START: char = '\u{4e00}';
const CJK_END: char = '\u{9fff}';

/// 追加到 system prompt 末尾的术语表说明。
pub fn prompt_block(terms: &[String]) -> String {
    let list = terms
        .iter()
        .map(|term| term.trim())
        .filter(|term| !term.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if list.is_empty() {
        return String::new();
    }
    format!(
        "\n\n用户指定术语表（以下词是标准写法。只有当原文中的词与术语表同音或近音，\
         且上下文支持时，才改成术语表写法。不要为了使用术语表而插入新词。\
         不要改动术语表以外的内容）：\n{list}"
    )
}

/// 返回输出里第一个没有同音来源的术语表词。
///
/// 输入里存在同音且等长的片段时视为合理纠错，不拦。术语表词已经出现在输入里时
/// 也不拦。只检查中文术语。
pub fn unsupported_insertion(input: &str, output: &str, terms: &[String]) -> Option<String> {
    if output.is_empty() {
        return None;
    }
    let input_pinyin = pinyin_sequence(input);
    for term in terms {
        let term = term.trim();
        if !is_chinese_term(term) {
            continue;
        }
        if input.contains(term) || !output.contains(term) {
            continue;
        }
        let char_count = term.chars().count();
        let mut term_pinyin = Vec::with_capacity(char_count);
        for character in term.chars() {
            match character.to_pinyin() {
                Some(pinyin) => term_pinyin.push(pinyin.plain()),
                None => {
                    term_pinyin.clear();
                    break;
                }
            }
        }
        if term_pinyin.len() != char_count {
            continue;
        }
        let found = input_pinyin.windows(char_count).any(|window| {
            window
                .iter()
                .zip(term_pinyin.iter())
                .all(|(input_pinyin, term_pinyin)| *input_pinyin == Some(*term_pinyin))
        });
        if !found {
            return Some(term.to_string());
        }
    }
    None
}

fn is_chinese_term(term: &str) -> bool {
    let mut count = 0;
    for character in term.chars() {
        if !(CJK_START..=CJK_END).contains(&character) {
            return false;
        }
        count += 1;
    }
    count >= 2
}

fn pinyin_sequence(text: &str) -> Vec<Option<&'static str>> {
    text.chars()
        .map(|character| character.to_pinyin().map(|pinyin| pinyin.plain()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terms(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn prompt_block_lists_terms_after_instruction() {
        let block = prompt_block(&terms(&["地月空间", "平动点"]));

        assert!(block.contains("用户指定术语表"));
        assert!(block.contains("地月空间\n平动点"));
        assert!(block.contains("不要为了使用术语表而插入新词"));
    }

    #[test]
    fn allows_homophone_correction() {
        assert_eq!(
            unsupported_insertion(
                "地约空间动力学分区",
                "地月空间动力学分区",
                &terms(&["地月空间"]),
            ),
            None
        );
    }

    #[test]
    fn allows_short_homophone_correction() {
        assert_eq!(
            unsupported_insertion("平洞点", "平动点", &terms(&["平动点"])),
            None
        );
    }

    #[test]
    fn allows_term_already_in_input() {
        assert_eq!(
            unsupported_insertion(
                "地月空间动力学",
                "地月空间动力学分区",
                &terms(&["地月空间"]),
            ),
            None
        );
    }

    #[test]
    fn rejects_unsupported_term_insertion() {
        assert_eq!(
            unsupported_insertion(
                "航天飞机的姿态需要控制。",
                "航天器的姿态需要控制。",
                &terms(&["航天器"]),
            ),
            Some("航天器".to_string())
        );
    }

    #[test]
    fn rejects_phrase_insertion_without_homophone() {
        assert_eq!(
            unsupported_insertion(
                "地月拉格朗日力学是研究重点。",
                "地月拉格朗日点力学是研究重点。",
                &terms(&["拉格朗日点"]),
            ),
            Some("拉格朗日点".to_string())
        );
    }

    #[test]
    fn skips_non_chinese_terms() {
        assert_eq!(
            unsupported_insertion("近直线晕轨道", "NRHO 轨道", &terms(&["NRHO"])),
            None
        );
    }
}
