//! 文本润色模块。
//!
//! 使用 LLM 对语音识别结果进行后期处理，支持 4 个润色级别：
//!
//! - `none`：不润色，直接返回原文
//! - `light`：修复标点和明显错别字
//! - `medium`：修复标点、错别字和语病，使语句更通顺
//! - `heavy`：重写为结构清晰、表达准确的文字
//!
//! 当语言为 `zh` 时，内置系统提示会约束输出为规范简体中文，并合并：材料概括类写作要求，以及本地安装的
//! ljg-writes 与 ljg-plain（lijigang/ljg-skills）中与“口语文本润色”相关的取向（非全文摘抄 skill 文件）。
//!
//! 使用兼容 OpenAI 的聊天 API，支持指数退避重试（最多 3 次）。
//!
//! 子模块按职责划分：`level` 层级枚举、`prompt` 系统提示与来源链、`endpoint` 地址推导、
//! `thinking` 思考控制字段、`cleanup` 思维链残渣清理、`formatter` 客户端与润色入口、
//! `transport` 请求发送与重试、`connectivity` 设置页连接测试。

pub mod protocol;

mod cleanup;
mod connectivity;
mod endpoint;
mod formatter;
mod level;
mod prompt;
mod thinking;
mod transport;

pub use connectivity::{describe_test_error, test_connection};
pub use endpoint::build_endpoint;
pub use formatter::LLMFormatter;
pub use level::{PolishLevel, ThinkingLevel};
pub use prompt::{CustomSource, PromptStoreSource, SystemPromptSource};
