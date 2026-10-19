# 在线转写按协议家族支持多家云端 ASR

在线转写此前只接小米 MiMo 一家（`MimoAsr`）。持有其他家 API Key（智谱、OpenAI、Groq、SiliconFlow、百炼等）的用户无法使用在线识别，只能退回本地模型，低配机器上体验差。

## 决策

- **按协议家族泛化，不按厂商适配。** 云端 ASR 端点按请求/响应形状归为三个家族，同一实现内分派，不为每家厂商写独立适配器：
  - `mimo_chat`（缺省）：chat/completions + input_audio，携带 MiMo 网关私有 `asr_options` 扩展（网关不接受文本块，语言经 `asr_options.language` 传递，空值发 `"auto"`）。
  - `chat_input_audio`：不带 `asr_options` 的纯 OpenAI 形状（百炼 qwen3-asr-flash 的 OpenAI 兼容模式、OpenAI audio-preview）。语言映射现有 `[transcriber] language` 配置，非空时作为顶层字段携带，空值不传（由服务端自动检测）。
  - `transcriptions`：OpenAI `/audio/transcriptions` multipart（`file` + `model`，可选 `language`），响应取顶层 `text` 字段（whisper 系、智谱 glm-asr、SiliconFlow SenseVoiceSmall 云端版）。
- `MimoAsr` 改名 `OnlineAsr`，MiMo 退为缺省协议；旧配置不含 `protocol` 字段时回退 `mimo_chat`，升级不破坏现状。
- `api_base_url`、`model`、`api_key`、`timeout` 复用现有字段；端点推导与润色器同源（`build_endpoint` 规则：base 无路径补 `/v1`，已含版本段只补请求路径），transcriptions 族按 base 补 `/audio/transcriptions`。
- 配置校验拒绝非法 `protocol` 值，`backend = "online"` 的必填项检查口径不变。
- 设置页加静态服务商预设下拉（前端表，不进配置结构），选中自动填 base URL、默认模型与协议，字段仍可手改；预设覆盖 MiMo、智谱、OpenAI、Groq、SiliconFlow。
- 不接 DashScope 异步任务协议（强制公网 URL 输入，与按住说话即传即转的产品形态冲突）；不做 MiniMax、Deepgram、Azure 私有协议与 Anthropic 协议（无音频输入能力）；不做流式（SSE）输出。Kimi、DeepSeek 无 ASR 的结论查证于 2026-10-19，接新厂商前先复核官方文档。

## 兼容清单（实测状态）

| 服务商 | 端点 | 协议 | 状态 |
|---|---|---|---|
| 小米 MiMo | `https://token-plan-cn.xiaomimimo.com/v1` | `mimo_chat` | 已实测 |
| 智谱 | `https://open.bigmodel.cn/api/paas/v4`（glm-asr-2512，30 秒与 25 MB 限制） | `transcriptions` | 未实测，拿到 Key 后补充 |
| OpenAI | `https://api.openai.com/v1`（whisper 系） | `transcriptions` | 未实测 |
| Groq | `https://api.groq.com/openai/v1`（whisper-large-v3） | `transcriptions` | 未实测 |
| SiliconFlow | `https://api.siliconflow.cn/v1`（FunAudioLLM/SenseVoiceSmall） | `transcriptions` | 未实测 |
| 百炼 | `https://dashscope.aliyuncs.com/compatible-mode/v1`（qwen3-asr-flash） | `chat_input_audio` | 未实测 |

## 后果

- 在线转写实现从"单厂商 SDK 味"变为协议分派器，新增家族只需新协议值与对应请求/响应形状，不再新增模块。
- `mimo_chat` 与 `chat_input_audio` 的语言语义不同（`asr_options.language` 恒发、顶层 `language` 空值不传），文档与 UI 文案需要向用户说明"空语言 = 服务端自动检测"。
- 未实测端点按 OpenAI 规范镜像实现，行为以实测为准，清单随实测更新。
