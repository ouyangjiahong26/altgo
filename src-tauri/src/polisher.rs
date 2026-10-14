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
mod fidelity;
mod formatter;
mod level;
mod local;
mod prompt;
mod thinking;
mod transport;

use crate::error::PolisherError;
use std::future::Future;
use std::pin::Pin;

pub use connectivity::{describe_test_error, test_connection};
pub use endpoint::build_endpoint;
pub use formatter::LLMFormatter;
pub use level::{PolishLevel, ThinkingLevel};
pub(crate) use local::LocalPolisher;
pub use prompt::{CustomSource, PromptStoreSource, SystemPromptSource};

/// 润色引擎：远端 API 与本地运行时两种实现。
///
/// 流水线与 IPC handler 只依赖这个接口，`[polisher] backend` 决定构造哪一种。
pub trait Polisher: Send + Sync {
    /// 按档位润色文本。档位为 `None` 或文本为空时返回原文。
    fn polish<'a>(
        &'a self,
        text: &'a str,
        level: PolishLevel,
    ) -> Pin<Box<dyn Future<Output = Result<String, PolisherError>> + Send + 'a>>;

    /// 润色并追加用户补充要求。
    fn polish_with_instruction<'a>(
        &'a self,
        text: &'a str,
        level: PolishLevel,
        instruction: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<String, PolisherError>> + Send + 'a>>;

    /// 回收空闲资源：本地引擎据此在空闲超时后关掉运行时进程，远端无事可做。
    fn release_idle<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<bool, PolisherError>> + Send + 'a>> {
        Box::pin(async { Ok(false) })
    }

    /// 流水线停止时释放资源（本地引擎在这里关掉运行时进程）。
    fn shutdown<'a>(&'a self) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async {})
    }
}

impl Polisher for LLMFormatter {
    fn polish<'a>(
        &'a self,
        text: &'a str,
        level: PolishLevel,
    ) -> Pin<Box<dyn Future<Output = Result<String, PolisherError>> + Send + 'a>> {
        Box::pin(async move { LLMFormatter::polish(self, text, level).await })
    }

    fn polish_with_instruction<'a>(
        &'a self,
        text: &'a str,
        level: PolishLevel,
        instruction: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<String, PolisherError>> + Send + 'a>> {
        Box::pin(async move {
            LLMFormatter::polish_with_instruction(self, text, level, instruction).await
        })
    }
}

/// 按配置构造润色引擎。
///
/// 实时流水线与历史重润都走这里，保证两条路径用的是同一个后端。
pub fn build_polisher(cfg: &crate::config::Config) -> Result<Box<dyn Polisher>, PolisherError> {
    match cfg.polisher.backend.trim().to_lowercase().as_str() {
        "local" => Ok(Box::new(LocalPolisher::new(cfg)?)),
        _ => Ok(Box::new(LLMFormatter::from_config_with_sources(cfg)?)),
    }
}
