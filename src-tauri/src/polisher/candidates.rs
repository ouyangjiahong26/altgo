//! 候选术语筛选：提示词、响应解析与结果过滤。
//!
//! 候选筛选是用户显式触发的短抽取，不是润色。它只发用户点开的那一条历史
//! 条目，输出一行一个候选，标准写法与原文片段用制表符分隔。AI 只给建议，
//! 是否加入术语表由用户决定。

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// 一条候选术语：标准写法与 AI 认为可能被误识的原文片段。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GlossaryCandidate {
    pub term: String,
    pub fragment: String,
}

/// 单次候选请求最多保留的条数。一条历史通常只有几个术语，20 条足够覆盖，
/// 也能把面板高度与请求成本控制在可预期范围。
pub const MAX_CANDIDATES: usize = 20;

/// 候选请求的 `max_tokens`。一行候选很短，512 个 token 足够 20 行两列文本。
pub const CANDIDATE_MAX_TOKENS: u32 = 512;

/// 候选请求的 system prompt。
///
/// 写成常量而不是放进 `resources/prompts/`：那份资源没有随安装包分发，
/// 放进去只会在开发版生效，装好的应用会退回另一套行为。
const SYSTEM_PROMPT: &str = "\
你是中文语音转写的术语校对助手。只找出可能因为同音或近音被误识的中文术语，\
不要改写句子，不要输出解释。每行输出一个候选，先写标准写法，再用一个制表符\
分隔，最后写原文片段。原文片段必须来自用户提供的文本。只输出标准写法，\
不要输出变体，不要编号，不要 Markdown，不要引号。没有候选时输出空。最多 20 条。";

/// 构造候选请求的 system prompt 与 user message。
///
/// `text` 与 `raw` 相同或为空时不重复发送。两者不同时标明关系，
/// 让模型能利用润色前后的差异判断哪些词可能被听错。
pub fn build_request(raw: &str, text: &str) -> (String, String) {
    let raw = raw.trim();
    let text = text.trim();
    let mut user = format!("原始转写：\n{raw}");
    if !text.is_empty() && text != raw {
        user.push_str("\n\n当前文本：\n");
        user.push_str(text);
    }
    (SYSTEM_PROMPT.to_string(), user)
}

/// 解析模型返回的候选行。
///
/// 容忍编号、项目符号、包裹用的引号或反引号，以及只有标准写法没有片段的行。
/// 丢弃空行、超长、重复、片段与标准写法相同的项，最多保留 `MAX_CANDIDATES` 条。
pub fn parse_response(raw: &str) -> Vec<GlossaryCandidate> {
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    for line in raw.lines() {
        let line = strip_list_marker(line.trim());
        if line.is_empty() {
            continue;
        }
        let (term, fragment) = match line.split_once('\t') {
            Some((term, fragment)) => (term.trim(), fragment.trim()),
            None => (line, ""),
        };
        let term = trim_wrapping(term);
        let fragment = trim_wrapping(fragment);
        if term.is_empty() || fragment == term {
            continue;
        }
        if term.chars().count() > crate::config::MAX_GLOSSARY_TERM_CHARS {
            continue;
        }
        if !seen.insert(term.to_string()) {
            continue;
        }
        candidates.push(GlossaryCandidate {
            term: term.to_string(),
            fragment: fragment.to_string(),
        });
        if candidates.len() >= MAX_CANDIDATES {
            break;
        }
    }
    candidates
}

/// 去掉已在术语表中的候选，避免面板展示用户已经加过的词。
pub fn retain_new(
    candidates: Vec<GlossaryCandidate>,
    glossary: &[String],
) -> Vec<GlossaryCandidate> {
    let existing: HashSet<&str> = glossary.iter().map(|term| term.trim()).collect();
    candidates
        .into_iter()
        .filter(|candidate| !existing.contains(candidate.term.as_str()))
        .collect()
}

/// 片段在原始转写与当前文本里都找不到时只保留标准写法。
///
/// 模型偶尔会改写片段，展示一个原文里不存在的片段会让用户无法核对。
pub fn resolve_fragments(
    mut candidates: Vec<GlossaryCandidate>,
    raw: &str,
    text: &str,
) -> Vec<GlossaryCandidate> {
    for candidate in &mut candidates {
        if !candidate.fragment.is_empty()
            && !raw.contains(&candidate.fragment)
            && !text.contains(&candidate.fragment)
        {
            candidate.fragment.clear();
        }
    }
    candidates
}

/// 去掉行首的列表标记。数字后必须跟常见分隔符才当作编号，避免把 `3D打印`
/// 这类以数字开头的术语截断。
fn strip_list_marker(line: &str) -> &str {
    for marker in ["- ", "* ", "• "] {
        if let Some(rest) = line.strip_prefix(marker) {
            return rest.trim_start();
        }
    }
    // 括号编号：（1）术语、(1) 术语。先剥括号，再看里面是不是纯数字。
    for (open, close) in [('（', '）'), ('(', ')')] {
        if let Some(rest) = line.strip_prefix(open) {
            let digit_len = rest.chars().take_while(|c| c.is_ascii_digit()).count();
            if digit_len > 0 {
                if let Some(rest) = rest[digit_len..].strip_prefix(close) {
                    return rest.trim_start();
                }
            }
        }
    }
    let digit_len = line.chars().take_while(|c| c.is_ascii_digit()).count();
    if digit_len == 0 {
        return line;
    }
    let rest = &line[digit_len..];
    // 1)术语、1）术语：右括号本身就是编号的结尾，不需要额外空格。
    for marker in [')', '）'] {
        if let Some(rest) = rest.strip_prefix(marker) {
            return rest.trim_start();
        }
    }
    for marker in ["、", "．", ") ", "） "] {
        if let Some(rest) = rest.strip_prefix(marker) {
            return rest.trim_start();
        }
    }
    if let Some(rest) = rest.strip_prefix(". ") {
        return rest.trim_start();
    }
    if let Some(rest) = rest.strip_prefix('.') {
        let next = rest.chars().next();
        if next
            .map(|c| c.is_whitespace() || is_cjk(c))
            .unwrap_or(false)
        {
            return rest.trim_start();
        }
    }
    line
}

/// 去掉模型可能包裹在字段外层的反引号与引号。
fn trim_wrapping(value: &str) -> &str {
    value
        .trim()
        .trim_matches(|c| matches!(c, '`' | '"' | '\'' | '“' | '”' | '‘' | '’' | '《' | '》'))
}

fn is_cjk(character: char) -> bool {
    ('\u{4e00}'..='\u{9fff}').contains(&character)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_reads_tab_separated_term_and_fragment() {
        let parsed = parse_response("地月空间\t地约空间\n平动点\t平洞点");

        assert_eq!(
            parsed,
            vec![
                GlossaryCandidate {
                    term: "地月空间".to_string(),
                    fragment: "地约空间".to_string(),
                },
                GlossaryCandidate {
                    term: "平动点".to_string(),
                    fragment: "平洞点".to_string(),
                },
            ]
        );
    }

    #[test]
    fn parse_tolerates_list_markers_and_missing_fragments() {
        let parsed = parse_response("1. 地月空间\t地约空间\n- 平动点\n* 近月空间\t\n");

        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].term, "地月空间");
        assert_eq!(parsed[1].term, "平动点");
        assert_eq!(parsed[1].fragment, "");
        assert_eq!(parsed[2].term, "近月空间");
    }

    #[test]
    fn parse_trims_wrapping_quotes_and_backticks() {
        let parsed = parse_response("`地月空间`\t“地约空间”\n");

        assert_eq!(parsed[0].term, "地月空间");
        assert_eq!(parsed[0].fragment, "地约空间");
    }

    #[test]
    fn parse_discards_invalid_duplicate_and_identical_fragments() {
        let long_term = "长".repeat(crate::config::MAX_GLOSSARY_TERM_CHARS + 1);
        let output = format!(
            "地月空间\t地约空间\n地月空间\t地阅空间\n地月空间\t地月空间\n{long_term}\t长片段\n\n"
        );

        let parsed = parse_response(&output);

        assert_eq!(
            parsed,
            vec![GlossaryCandidate {
                term: "地月空间".to_string(),
                fragment: "地约空间".to_string(),
            }]
        );
    }

    #[test]
    fn parse_caps_at_twenty_candidates() {
        let output = (0..25)
            .map(|index| format!("术语{index}\t原文{index}"))
            .collect::<Vec<_>>()
            .join("\n");

        assert_eq!(parse_response(&output).len(), MAX_CANDIDATES);
    }

    #[test]
    fn build_request_labels_both_texts_when_they_differ() {
        let (system, user) = build_request("地约空间", "地月空间");

        assert!(system.contains("候选术语") || system.contains("术语校对"));
        assert!(user.contains("原始转写"));
        assert!(user.contains("当前文本"));
        assert!(user.contains("地约空间"));
        assert!(user.contains("地月空间"));
    }

    #[test]
    fn build_request_omits_current_text_when_identical() {
        let (_, user) = build_request("地约空间", " 地约空间 ");

        assert!(user.contains("原始转写"));
        assert!(!user.contains("当前文本"));
    }

    #[test]
    fn retain_new_removes_terms_already_in_glossary() {
        let candidates = vec![
            GlossaryCandidate {
                term: "地月空间".to_string(),
                fragment: "地约空间".to_string(),
            },
            GlossaryCandidate {
                term: "平动点".to_string(),
                fragment: "平洞点".to_string(),
            },
        ];
        let glossary = vec![" 地月空间 ".to_string()];

        let filtered = retain_new(candidates, &glossary);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].term, "平动点");
    }

    #[test]
    fn resolve_fragments_blanks_fragments_not_present_in_either_text() {
        let candidates = vec![
            GlossaryCandidate {
                term: "地月空间".to_string(),
                fragment: "地约空间".to_string(),
            },
            GlossaryCandidate {
                term: "平动点".to_string(),
                fragment: "原文没有的片段".to_string(),
            },
        ];

        let resolved = resolve_fragments(candidates, "地约空间", "地月空间");

        assert_eq!(resolved[0].fragment, "地约空间");
        assert_eq!(resolved[1].fragment, "");
    }

    #[test]
    fn strip_list_marker_keeps_terms_starting_with_digits() {
        assert_eq!(strip_list_marker("3D打印"), "3D打印");
        assert_eq!(strip_list_marker("1. 地月空间"), "地月空间");
        assert_eq!(strip_list_marker("1、地月空间"), "地月空间");
    }

    #[test]
    fn strip_list_marker_handles_parenthesized_and_bracket_numbering() {
        assert_eq!(strip_list_marker("1)地月空间"), "地月空间");
        assert_eq!(strip_list_marker("1）地月空间"), "地月空间");
        assert_eq!(strip_list_marker("(1) 地月空间"), "地月空间");
        assert_eq!(strip_list_marker("（1）地月空间"), "地月空间");
        // 数字开头的普通术语不能被误伤。
        assert_eq!(strip_list_marker("3.5mm 接头"), "3.5mm 接头");
    }
}
