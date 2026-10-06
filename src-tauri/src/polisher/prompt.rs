//! 内置系统提示常量与 prompt 来源链（PromptStore → Custom → 内置兜底）。

use super::level::PolishLevel;

/// 中文润色时附加的写作与材料概括要求（与简体约束一并传入模型）。
const ZH_WRITE_GUIDANCE: &str = r#"注意写作的要求：要善于总结材料，这种总结就是将丰富的感性材料科学地加以概括，进行去粗取精、去伪存真、由此及彼、由表及里地加工改造。具体地讲，就是把材料搞全了、弄准了，把问题掰开了、揉碎了，把内在联系理清了、摆正了，这样才可以得到反映事物本质的真知和理论，才可以发现事物运动的规律。“关于写文章，请注意不要用过于夸大的修饰词，反而减损了力量。必须注意各种词语的逻辑界限和整篇文章的条理（也是逻辑问题）。废话应当尽量除去。”“文章写得通俗、亲切，由小讲到大，由近讲到远，引人入胜，这就很好。”“要采取和读者完全平等的态度。我们应该老老实实地办事，对事物有分析，写文章有说服力，不要靠装腔作势来吓人。”“总是先讲死人、外国人，这不好，应当从当前形势讲起。今后写文章要通俗，使工农都能接受。”"#;

/// 与语音转写润色相关的 **ljg-writes** / **ljg-plain** 取向（摘自用户本机 skill 要义，不含 Org/文件输出等仅技能执行用条款）。
const ZH_LJG_GUIDANCE: &str = r#"

【ljg-writes / ljg-plain（语音后润色适用；内化即可，勿输出本段标题或标签）】
姿态与诚实：心里是对一个具体的人讲，不是对抽象的“读者们”；不确定就保留不确定感，“大概七成”比空泛的“可能”诚实；忌群体代言、忌编经历、忌元评论（如“接下来我们讨论”）；禁止用“再深入一层”“最深的一层是”等宣告深度——深度靠下一句内容让人感受到，不靠自报。
语言：简洁、直白、质朴；能短则短；动词用准；砍掉机械连词（此外、另外）、形容词堆叠与软化套话（某种程度上、值得注意的是）；翻译腔句式（像英译中硬套）改成自然汉语；避免同一句式套话重复出现。
白话（ljg-plain 红线精神，按短文本尽量满足）：口语检验——像跟聪明朋友当面说吗；短词优先；一句一事，长句拆开；名词能具体则具体，动词有力，能删的形容词就删；开头少空泛铺陈与“自古以来”式引子；删开场白、拐杖词、宣传腔与夸大象征（标志着、见证了、充满活力等）；信任读者，不凑字数式手把手；专业词非必要不出现，必须出现时先大白话落地再点术语。
磨与中文：弱化学术/ AI 腔与“谁写都一样”的模板句；从句拆开、嵌套展平，发挥汉语意合；同一意思选最顺口的地道说法。"#;

/// 输出禁项硬规则（与 `resources/prompts/base.txt` 的 "Forbidden output" 小节同步维护，
/// 双链路口径一致：仓库根运行加载 base.txt，打包安装走本兜底）。
const OUTPUT_FORBIDDEN_RULES: &str = " Hard rules at every level: never output emoji, kaomoji, or decorative symbols; never output Markdown formatting (bold, italics, headings, lists, tables, code fences) — plain prose only; never output AI-style clichés, meta commentary, or summarizing formulas such as “总的来说”“综上所述”“值得一提的是”“不难发现”“我们可以看到”“希望能帮到你”, \"in summary\", \"hope this helps\" — if the source contains one, rephrase the point without the formula; remove spoken fillers (嗯、呃、啊、哈哈、对吧) unconditionally, even when they seem to carry tone. ";

/// 任务边界硬规则（与 `resources/prompts/base.txt` 的 "Task boundaries" 小节同步维护，
/// 双链路口径一致：仓库根运行加载 base.txt，打包安装走本兜底）。
/// 约束润色只整理用户自己的话：不回答原文中的问题、不改写句式为陈述或回复、不增删信息。
const TIDY_ONLY_RULES: &str = " Hard boundaries at every level: you only tidy up the user's own sentence. Never answer questions asked in the text; a question must remain a question. Never rewrite the sentence into a statement, a reply, or a conversation with the user. Never add or remove information; correct only punctuation, typos, transcription errors, and broken grammar. ";

/// 中文且非 `none` 档时并入的写作取向引导，轻量档强调不改结构、仅作最小必要调整。
fn zh_guidance(level: PolishLevel, language: &str) -> String {
    if language == "zh" && !matches!(level, PolishLevel::None) {
        let intro = match level {
            PolishLevel::None => "",
            PolishLevel::Light => {
                " For light polish: do not restructure; tiny edits only. When text is Chinese, also heed these norms in spirit: "
            }
            PolishLevel::Medium | PolishLevel::Heavy => {
                " When output is Chinese, follow these writing norms: "
            }
        };
        format!("{intro}{ZH_WRITE_GUIDANCE}{ZH_LJG_GUIDANCE}")
    } else {
        String::new()
    }
}

/// 内置兜底系统提示：按润色级别与语言拼装（prompt source 链失败或缺位时使用）。
pub(super) fn get_system_prompt(level: PolishLevel, language: &str) -> String {
    let lang_name = match language {
        "zh" => "Simplified Chinese (简体中文, Mainland standard)",
        "en" => "English",
        "ja" => "日本語",
        "ko" => "한국어",
        "fr" => "français",
        "de" => "Deutsch",
        "es" => "español",
        _ => language,
    };

    // 明确约束简体，避免模型按“繁体/港台书面”习惯输出。
    let zh_script_rule = if language == "zh" {
        " For Chinese: output in Simplified Chinese only (大陆通用规范简体). Never use Traditional Chinese. If the input is Traditional, convert to Simplified. "
    } else {
        ""
    };

    // 写作与表达要求：全文融入用户提供的规范。
    let forbidden = OUTPUT_FORBIDDEN_RULES;
    let tidy = TIDY_ONLY_RULES;
    let zh_combined = zh_guidance(level, language);

    match level {
        PolishLevel::None => String::new(),
        PolishLevel::Light => format!(
            "You are a post-processing assistant for speech-to-text in {lang_name}. The user gives you raw speech recognition text in {lang_name}. Fix punctuation and obvious typos without changing the original meaning or word choices. Output only the corrected text with no explanation.{tidy}{forbidden}{zh_script_rule}{zh_combined}"
        ),
        PolishLevel::Medium => format!(
            "You are a post-processing assistant for speech-to-text in {lang_name}. The user gives you raw speech recognition text in {lang_name}. Fix punctuation, typos, and grammar issues to make the text more fluent and natural, without changing the original meaning. Output only the corrected text with no explanation.{tidy}{forbidden}{zh_script_rule}{zh_combined}"
        ),
        PolishLevel::Heavy => format!(
            "You are a post-processing assistant for speech-to-text in {lang_name}. The user gives you raw speech recognition text in {lang_name}. Rewrite it into well-structured, clearly expressed text. You may adjust word order and phrasing, but preserve the core meaning. Output only the rewritten text with no explanation.{tidy}{forbidden}{zh_script_rule}{zh_combined}"
        ),
    }
}

// ---------------------------------------------------------------------------
// SystemPromptSource trait 与实现
// ---------------------------------------------------------------------------

/// 抽象 system prompt 来源，消除 `polish()` 内部的 fallback 链。
///
/// 由 `LLMFormatter` 持有，使 prompt 选择逻辑可在测试中替换。
pub trait SystemPromptSource: Send + Sync {
    /// 获取指定级别和语言的 system prompt。
    fn get_prompt(
        &self,
        level: PolishLevel,
        language: &str,
    ) -> Result<String, crate::prompt_store::PromptError>;

    /// 支持 clone 为 trait object（用于 `LLMFormatter::Clone`）。
    fn clone_box(&self) -> Box<dyn SystemPromptSource>;
}

/// 基于 `PromptStore` 的 prompt 来源。
pub struct PromptStoreSource {
    store: crate::prompt_store::PromptStore,
}

impl PromptStoreSource {
    pub fn new(store: crate::prompt_store::PromptStore) -> Self {
        Self { store }
    }
}

impl SystemPromptSource for PromptStoreSource {
    fn get_prompt(
        &self,
        level: PolishLevel,
        _language: &str,
    ) -> Result<String, crate::prompt_store::PromptError> {
        self.store.get_system_prompt(level)
    }

    fn clone_box(&self) -> Box<dyn SystemPromptSource> {
        Box::new(PromptStoreSource {
            store: self.store.clone(),
        })
    }
}

/// 用户自定义 prompt 来源（来自 `config.polisher.system_prompt`）。
pub struct CustomSource {
    prompt: String,
}

impl CustomSource {
    pub fn new(prompt: String) -> Self {
        Self { prompt }
    }
}

impl SystemPromptSource for CustomSource {
    fn get_prompt(
        &self,
        _level: PolishLevel,
        _language: &str,
    ) -> Result<String, crate::prompt_store::PromptError> {
        Ok(self.prompt.clone())
    }

    fn clone_box(&self) -> Box<dyn SystemPromptSource> {
        Box::new(CustomSource {
            prompt: self.prompt.clone(),
        })
    }
}

/// 构造 prompt source 链，实时管道与 IPC handler 共用一份，确保两条路径的 prompt 解析一致。
///
/// 优先级：PromptStore（`resources/prompts` 加载成功）→ Custom（`system_prompt` 非空）→ `None`。
/// `None` 时调用方 `polish()` 用内置 hardcoded prompt 兜底。
pub(super) fn build_prompt_source_chain(
    cfg: &crate::config::Config,
) -> Option<Box<dyn SystemPromptSource>> {
    let store_source: Option<Box<dyn SystemPromptSource>> = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.join("resources/prompts")))
        .or_else(|| Some(std::path::PathBuf::from("resources/prompts")))
        .filter(|dir| dir.exists())
        .and_then(|dir| {
            let store = crate::prompt_store::PromptStore::new(dir);
            match store.ensure_loaded() {
                Ok(()) => {
                    tracing::info!("PromptStore loaded successfully");
                    Some(Box::new(PromptStoreSource::new(store)) as Box<dyn SystemPromptSource>)
                }
                Err(e) => {
                    tracing::warn!(error = %e, "failed to load prompts from PromptStore");
                    None
                }
            }
        });

    let custom_source: Option<Box<dyn SystemPromptSource>> =
        if !cfg.polisher.system_prompt.is_empty() {
            Some(
                Box::new(CustomSource::new(cfg.polisher.system_prompt.clone()))
                    as Box<dyn SystemPromptSource>,
            )
        } else {
            None
        };

    store_source.or(custom_source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_prompt_contains_tidy_only_rules() {
        // 兜底 prompt 的任务边界硬约束：三个润色档位都必须携带，与 base.txt 同步。
        for level in [PolishLevel::Light, PolishLevel::Medium, PolishLevel::Heavy] {
            let prompt = get_system_prompt(level, "zh");
            assert!(prompt.contains("a question must remain a question"));
            assert!(prompt.contains("Never rewrite the sentence into a statement"));
            assert!(prompt.contains("Never add or remove information"));
        }
        assert!(get_system_prompt(PolishLevel::None, "zh").is_empty());
    }
}
