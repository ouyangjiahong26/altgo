//! 润色结果的思维链残渣清理。

/// ASCII 大小写不敏感的字串查找（返回字节下标）。
fn find_ascii_ci(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .as_bytes()
        .windows(needle.len())
        .position(|w| w.eq_ignore_ascii_case(needle.as_bytes()))
}

/// 剥掉 `<think>…</think>` 块（含截断导致的未闭合块）。
///
/// 部分 OpenAI 兼容端点（尤其未命中抑制表的中转站）会把思维链以 think 块
/// 内联进正文，润色结果不应混入这些残渣。
pub(crate) fn strip_thinking_tags(text: &str) -> String {
    let mut rest = text;
    let mut out = String::with_capacity(text.len());
    while let Some(start) = find_ascii_ci(rest, "<think>") {
        out.push_str(&rest[..start]);
        let after = &rest[start + "<think>".len()..];
        match find_ascii_ci(after, "</think>") {
            Some(end) => rest = &after[end + "</think>".len()..],
            // 未闭合块：剥到末尾
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_thinking_tags() {
        // 闭合块
        assert_eq!(strip_thinking_tags("<think>想一想</think>正文"), "正文");
        // 未闭合块（流式截断）：剥到末尾
        assert_eq!(strip_thinking_tags("正文<think>没写完"), "正文");
        // 多个块
        assert_eq!(
            strip_thinking_tags("<think>a</think>中<think>b</think>尾"),
            "中尾"
        );
        // 大小写不敏感
        assert_eq!(strip_thinking_tags("<THINK>x</THINK>ok"), "ok");
        // 无标签原样返回
        assert_eq!(strip_thinking_tags("没有标签"), "没有标签");
        // 全是 think 块则清空
        assert_eq!(strip_thinking_tags("<think>只有思考</think>"), "");
        assert_eq!(strip_thinking_tags(""), "");
    }
}
