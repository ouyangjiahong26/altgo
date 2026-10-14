//! 保真校验：本地档的润色输出在下发前过一遍硬规则。
//!
//! 小模型会过编辑：把专名译掉、把长句压成摘要、把口语里的内容补全成回答。四条硬规则
//! 只拦这些越界行为，不拦正常的整理（ADR-0011）。ASCII 单词变化只写日志不判失败，
//! 因为把 `lininux` 改成 `Linux` 正是期望的纠错。

use crate::error::PolisherError;
use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;

/// 内容长度比的允许区间（下界挡压缩成摘要，上界挡自行扩写）。
const MIN_LENGTH_RATIO: f32 = 0.5;
const MAX_LENGTH_RATIO: f32 = 1.6;

/// 输出里未在输入出现过的汉字占比上限。
const MAX_NOVEL_CJK_RATIO: f32 = 0.25;

/// 未出现汉字占比规则生效所需的最小输入汉字数，太短时该比例没有统计意义。
const NOVEL_CHECK_MIN_CJK: usize = 8;

/// 模板标记与推理残渣。
static TEMPLATE_LEAK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)<\|[^|]*\|>|</?s>|<think|</think|^\s*assistant\s*:").unwrap()
});

/// 表情符号与装饰符号。
static EMOJI: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\x{1F300}-\x{1FAFF}\x{2600}-\x{27BF}\x{FE0F}]").unwrap());

/// Markdown 格式：粗体、斜体、标题、列表、引用、代码块、表格。
static MARKDOWN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)(\*\*|__|```|^\s{0,3}#{1,6}\s|^\s{0,3}[-*+]\s|^\s{0,3}\d+\.\s|^\s{0,3}>\s|^\s*\|.*\|\s*$)")
        .unwrap()
});

/// AI 套话，与 `resources/prompts/base.txt` 的禁项一致。
static CLICHE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(总的来说|综上所述|值得一提的是|不难发现|我们可以看到|希望能帮到你|in summary|hope this helps)")
        .unwrap()
});

/// 必须逐字保留的实体：数字（含版本号与小数）、URL、含路径分隔符的串。
static NUMBERS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d+(?:[.:]\d+)*").unwrap());
static URLS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"https?://[^\s，。；、）)】]+").unwrap());
static PATHS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[A-Za-z0-9_.\-]*[\\/][A-Za-z0-9_.\-/\\]*[A-Za-z0-9_]").unwrap());

/// ASCII 单词，用于判定“期望的纠错”并记日志。
static ASCII_TOKEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[A-Za-z][A-Za-z0-9_.\-]*").unwrap());

/// 校验失败的具体原因。
#[derive(Debug, Clone, PartialEq)]
pub enum FidelityViolation {
    /// 输出为空。
    EmptyOutput,
    /// 必须逐字保留的实体在输出里变了或丢了。
    MissingEntity(String),
    /// 内容长度比越界。
    LengthRatio(f32),
    /// 凭空造出的汉字占比过高。
    NovelChars(f32),
    /// 模板标记或推理残渣泄漏。
    TemplateLeak,
    /// 出现表情符号。
    Emoji,
    /// 出现 Markdown 格式。
    Markdown,
    /// 出现 AI 套话。
    Cliche,
}

impl std::fmt::Display for FidelityViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyOutput => write!(f, "输出为空"),
            Self::MissingEntity(entity) => write!(f, "实体未逐字保留：{entity}"),
            Self::LengthRatio(ratio) => write!(f, "内容长度比 {ratio:.2} 越界"),
            Self::NovelChars(ratio) => write!(f, "凭空多出的汉字占比 {ratio:.2} 过高"),
            Self::TemplateLeak => write!(f, "输出含模板标记或推理残渣"),
            Self::Emoji => write!(f, "输出含表情符号"),
            Self::Markdown => write!(f, "输出含 Markdown 格式"),
            Self::Cliche => write!(f, "输出含 AI 套话"),
        }
    }
}

/// 按四条硬规则检查一次润色输出。
pub fn check(input: &str, output: &str) -> Result<(), FidelityViolation> {
    let output = output.trim();
    if output.is_empty() {
        return Err(FidelityViolation::EmptyOutput);
    }
    if TEMPLATE_LEAK.is_match(output) {
        return Err(FidelityViolation::TemplateLeak);
    }
    if EMOJI.is_match(output) {
        return Err(FidelityViolation::Emoji);
    }
    if MARKDOWN.is_match(output) {
        return Err(FidelityViolation::Markdown);
    }
    if CLICHE.is_match(output) {
        return Err(FidelityViolation::Cliche);
    }

    for entity in required_entities(input) {
        if !contains_entity(output, entity.as_str()) {
            return Err(FidelityViolation::MissingEntity(entity));
        }
    }

    let (input_len, output_len) = (content_length(input), content_length(output));
    if input_len > 0 {
        let ratio = output_len as f32 / input_len as f32;
        if ratio < MIN_LENGTH_RATIO || ratio > MAX_LENGTH_RATIO {
            return Err(FidelityViolation::LengthRatio(ratio));
        }
    }

    let input_cjk = cjk_chars(input);
    if input_cjk.len() >= NOVEL_CHECK_MIN_CJK {
        let known: HashSet<char> = input_cjk.iter().copied().collect();
        let output_cjk = cjk_chars(output);
        if !output_cjk.is_empty() {
            let novel = output_cjk.iter().filter(|c| !known.contains(c)).count();
            let ratio = novel as f32 / output_cjk.len() as f32;
            if ratio > MAX_NOVEL_CJK_RATIO {
                return Err(FidelityViolation::NovelChars(ratio));
            }
        }
    }

    Ok(())
}

/// 校验并把失败映射成润色错误，成功时原样返回输出。
pub fn apply_guard(input: &str, output: &str) -> Result<String, PolisherError> {
    match check(input, output) {
        Ok(()) => {
            let changed = ascii_token_changes(input, output);
            if !changed.is_empty() {
                // 期望的纠错（把 lininux 改成 Linux）不判失败，只留一行痕迹便于排查。
                tracing::debug!(tokens = ?changed, "local polish changed ascii tokens");
            }
            Ok(output.to_string())
        }
        Err(violation) => {
            tracing::warn!(%violation, "local polish output rejected by fidelity guard");
            Err(PolisherError::FidelityGuard(violation.to_string()))
        }
    }
}

/// 实体是否在输出里逐字出现，且两端不与 ASCII 字母数字粘连。
///
/// 判定带词边界：输入里的 `3000` 不该因为输出是 `30000` 而被认为保留了。
fn contains_entity(output: &str, entity: &str) -> bool {
    let mut search_from = 0usize;
    while let Some(offset) = output[search_from..].find(entity) {
        let at = search_from + offset;
        if boundary_free(output, at, at + entity.len()) {
            return true;
        }
        search_from = at + entity.len();
    }
    false
}

/// 区间两端是否都不与 ASCII 字母数字粘连。中文与标点相邻不算粘连。
fn boundary_free(text: &str, start: usize, end: usize) -> bool {
    let glued = |c: char| c.is_ascii_alphanumeric();
    let before_free = !text[..start].chars().next_back().is_some_and(glued);
    let after_free = !text[end..].chars().next().is_some_and(glued);
    before_free && after_free
}

/// 输出里新出现的 ASCII 单词，供 `apply_guard` 记日志。
fn ascii_token_changes(input: &str, output: &str) -> Vec<String> {
    let known: HashSet<&str> = ASCII_TOKEN
        .find_iter(input)
        .map(|hit| hit.as_str())
        .collect();
    let mut changed: Vec<String> = Vec::new();
    for hit in ASCII_TOKEN.find_iter(output) {
        let token = hit.as_str();
        if !known.contains(token) && !changed.iter().any(|existing| existing == token) {
            changed.push(token.to_string());
        }
    }
    changed
}

/// 输入里必须逐字保留的实体：数字、URL、含路径分隔符的串，按在原文里的出现顺序返回。
///
/// 只收自成边界的匹配。标识符或网址内部的片段（`ouyangjiahong26` 里的 `26`）已经被外层
/// 词覆盖，单独再要求一次只会在输出里找不到而误判。
fn required_entities(input: &str) -> Vec<String> {
    let mut found: Vec<(usize, String)> = Vec::new();
    for pattern in [&*URLS, &*PATHS, &*NUMBERS] {
        for hit in pattern.find_iter(input) {
            if !boundary_free(input, hit.start(), hit.end()) {
                continue;
            }
            found.push((hit.start(), hit.as_str().to_string()));
        }
    }
    found.sort_by_key(|(start, _)| *start);

    let mut entities: Vec<String> = Vec::new();
    for (_, entity) in found {
        if !entities.contains(&entity) {
            entities.push(entity);
        }
    }
    entities
}

/// 内容长度：汉字数加字母数，混排文本也能比较。
fn content_length(text: &str) -> usize {
    text.chars()
        .filter(|c| is_cjk(*c) || c.is_ascii_alphabetic())
        .count()
}

fn cjk_chars(text: &str) -> Vec<char> {
    text.chars().filter(|c| is_cjk(*c)).collect()
}

fn is_cjk(c: char) -> bool {
    matches!(c, '\u{4E00}'..='\u{9FFF}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_polish_passes() {
        let input = "嗯那个，我们明天上午十点开会，呃，记得带上方案。";
        let output = "我们明天上午十点开会，记得带上方案。";
        assert!(check(input, output).is_ok());
    }

    /// 期望的纠错不算越界：ASCII 单词变化不拦。
    #[test]
    fn test_ascii_correction_is_allowed() {
        let input = "另外我想知道这个 lininux 测有有没有必要走这个设计这个抢救策略";
        let output = "另外我想知道这个 Linux 测试有没有必要走这个设计里的抢救策略";
        assert!(check(input, output).is_ok());
    }

    #[test]
    fn test_entities_must_survive_verbatim() {
        let input =
            "端口是 3000，仓库在 https://github.com/ouyangjiahong26/altgo，文件在 src/main.rs";
        assert!(check(
            input,
            "端口是 3000，仓库在 https://github.com/ouyangjiahong26/altgo，文件在 src/main.rs"
        )
        .is_ok());

        let error = check(input, "端口是 3306，仓库见项目主页，文件在 src/main.rs").unwrap_err();
        assert_eq!(error, FidelityViolation::MissingEntity("3000".to_string()));
        assert!(error.to_string().contains("3000"));
    }

    #[test]
    fn test_compressed_rewrite_is_rejected_but_mild_compression_passes() {
        let input = "刚刚那一句话嗯说错了，我的意思是为什么 Code 它可以自动识别版本，但是 AutoGo 不可以，而是需要自己创建一些配置";
        let mild = "这句话说错了，我的意思是 Code 可以自动识别版本，但 AutoGo 不可以，需要自己创建一些配置";
        assert!(check(input, mild).is_ok());

        let summary = "Code 可以自动识别版本，AutoGo 不行，需要配置";
        match check(input, summary).unwrap_err() {
            FidelityViolation::LengthRatio(ratio) => assert!(ratio < MIN_LENGTH_RATIO, "{ratio}"),
            other => panic!("应当判长度比越界，实际：{other:?}"),
        }
    }

    #[test]
    fn test_expansion_is_rejected() {
        let error = check("我。", "我有什么我可以帮你做的？").unwrap_err();
        match error {
            FidelityViolation::LengthRatio(ratio) => assert!(ratio > MAX_LENGTH_RATIO, "{ratio}"),
            other => panic!("应当判长度比越界，实际：{other:?}"),
        }
    }

    #[test]
    fn test_novel_chinese_is_rejected() {
        let input = "咱正是人要运行那种东西的时候";
        let output = "咱们现在正处在运行一种状态的时候，需要检查配置";
        match check(input, output).unwrap_err() {
            FidelityViolation::NovelChars(ratio) => assert!(ratio > MAX_NOVEL_CJK_RATIO, "{ratio}"),
            other => panic!("应当判凭空造字，实际：{other:?}"),
        }
    }

    #[test]
    fn test_format_leaks_are_rejected() {
        let input = "把这段话说清楚";
        for output in [
            "**重点**：把这段话说清楚",
            "- 把这段话说清楚",
            "# 整理结果\n把这段话说清楚",
            "```\n把这段话说清楚\n```",
            "| 项 | 值 |\n| --- | --- |\n| 一句 | 把这段话说清楚 |",
            "把这段话说清楚 😀",
            "总的来说，把这段话说清楚",
            "把这段话说清楚</s>",
        ] {
            let error = check(input, output).unwrap_err();
            assert!(
                matches!(
                    error,
                    FidelityViolation::Markdown
                        | FidelityViolation::Emoji
                        | FidelityViolation::Cliche
                        | FidelityViolation::TemplateLeak
                ),
                "“{output}”应当被判格式越界，实际：{error:?}"
            );
        }
    }

    #[test]
    fn test_empty_output_is_rejected() {
        assert_eq!(
            check("原文", "   ").unwrap_err(),
            FidelityViolation::EmptyOutput
        );
    }

    #[test]
    fn test_guard_maps_violation_to_polisher_error() {
        let error = apply_guard("我。", "我有什么我可以帮你做的？").unwrap_err();
        assert!(matches!(error, PolisherError::FidelityGuard(_)));
        assert!(error.to_string().contains("长度比"), "{error}");

        assert_eq!(apply_guard("原文。", "原文。").unwrap(), "原文。");
    }

    #[test]
    fn test_english_input_uses_letter_counts() {
        let input = "so uh we shipped the new build yesterday and it looks fine";
        let output = "We shipped the new build yesterday and it looks fine.";
        assert!(check(input, output).is_ok());
        assert!(check(input, "Shipped.").is_err(), "大幅压缩应当被拦");
    }

    /// 实体判定要带词边界：`3000` 出现在 `30000` 里不算保留。
    #[test]
    fn test_entity_match_requires_token_boundary() {
        assert!(check("端口是 3000，别记错。", "端口是 3000，别记错。").is_ok());
        let error = check("端口是 3000，别记错。", "端口是 30000，别记错。").unwrap_err();
        assert_eq!(error, FidelityViolation::MissingEntity("3000".to_string()));

        // 中文字符与标点不算粘连，贴在一起也应当命中。
        assert!(check("跑在 8080 上。", "跑在8080上。").is_ok());
    }

    #[test]
    fn test_embedded_fragments_are_covered_by_the_outer_token() {
        // 网址里的 26 不是独立实体，不该单独要求出现一次。
        let url = "https://github.com/ouyangjiahong26/altgo";
        assert!(check(url, url).is_ok());
        assert!(check(&format!("仓库在 {url}。"), &format!("见 {url}。")).is_ok());
    }

    #[test]
    fn test_ascii_token_changes_lists_only_new_tokens() {
        let changed = ascii_token_changes(
            "这个 lininux 版本和 Postgres 都要改。",
            "这个 Linux 版本和 PostgreSQL 都要改。",
        );
        assert!(changed.contains(&"Linux".to_string()), "{changed:?}");
        assert!(changed.contains(&"PostgreSQL".to_string()), "{changed:?}");
        assert_eq!(changed.len(), 2, "已存在的单词不应被报出来：{changed:?}");
        assert!(ascii_token_changes("原样。", "原样。").is_empty());
    }
}
