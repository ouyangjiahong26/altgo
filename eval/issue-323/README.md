# Issue 323 文本侧评估

验证用户自设术语表（只填标准写法）能否修正领域术语的同音误识。评估只处理文本，不录音，不修改生产代码。

## 范围

- 术语表由用户设置，只含标准写法，不要求用户填变体。
- 第一轮配置跟当前应用配置一致：本地 SenseVoice、远端 DeepSeek、`heavy` 档。
- `none` 档必须覆盖，拼音替换是主要候选路径。
- 指标在留出集上报告，开发集只用于调 prompt 与替换约束。
- 报告明确区分真实历史错例与合成定向句。合成句用于验证机制，不代表端到端 ASR 错误分布。

## 运行

```bash
cd eval/issue-323
uv run --with pypinyin --with requests python evaluate.py
```

也可以先建虚拟环境：

```bash
python3 -m venv .venv
.venv/bin/pip install -r requirements.txt
.venv/bin/python evaluate.py
```

常用参数：

| 参数 | 默认值 | 用途 |
|---|---|---|
| `--history` | `~/.config/altgo/history.json` | 历史条目，包含 `rawText` 与 `text` |
| `--config` | `~/.config/altgo/altgo.toml` | 远端润色配置，读取模型、端点、温度与 token 上限 |
| `--prompts` | `resources/prompts` | `base.txt` 与 `heavy-suffix.txt` |
| `--out` | `results` | 指标、缓存与普通句改动排名 |
| `--workers` | `6` | 远端请求并发数 |
| `--smoke` | 关 | 用 2 条定向、2 条硬负例、2 条普通句验证链路 |
| `--no-llm` | 关 | 只跑基线与替换，不调用远端 |

API Key 优先读 `ALTGO_POLISHER_API_KEY`，否则读配置里的 `[polisher] api_key`。脚本不打印 Key。

## 测试

```bash
cd eval/issue-323
uv run --with pypinyin --with requests python -m unittest discover -s tests
```

## 输入文件

| 文件 | 内容 |
|---|---|
| `glossaries/terms_200.txt` | 200 条航天与地月空间术语。前 10 条是主评估集，前 50 条是中间档 |
| `data/authored_directed.txt` | 30 条含目标术语的定向句，每条对应一个术语 |
| `data/term_variants.txt` | 精确替换上限对照用的手工变体表 |
| `data/hard_negatives.txt` | 60 条硬负例，含同音片段但片段本身正确 |
| `tests/` | 拼音匹配、变体替换、指标与语料的单元测试 |

## 方法

| 方法 | 说明 |
|---|---|
| `none` | 直接使用 `rawText`，对应 `none` 档 |
| `pinyin_10/50/200` | 从标准写法生成同音变体，在 `none` 档做最长匹配替换 |
| `exact_10` | 用手工变体表替换，只作上限对照，不代表用户输入 |
| `heavy_no_glossary` | 当前 `heavy` prompt，不加术语表 |
| `heavy_glossary_10/50/200` | 当前 `heavy` prompt 末尾追加术语表 |
| `combo_pinyin_10_heavy` | 先拼音替换，再走 `heavy` |
| `combo_pinyin_10_heavy_glossary_10` | 先拼音替换，再走带术语表的 `heavy` |

## 指标

- 术语修正率：期望术语出现在输出里的比例。
- 同音误替换率：硬负例里被同音片段误导、替换成术语的样本比例。
- 任何术语插入率：输出里出现输入没有的术语的样本比例，包含同义改写（如把月亮改成月球）。
- 非目标改动：普通句去掉标点与口头语后的字面差异，附前 30 条改动排名供人工复核。
- 实体保真：数字、URL、路径是否逐字保留。
- 附加延迟：p50 与 p95，相对 `none` 基线。
- token 成本：每千条请求的输入与输出 token。
- 稳定性：同一输入重复三次的结果一致率。

## 隐私

脚本在本机读取历史文件。只有评估样本会发给当前配置的远端润色服务，发送行为与用户平时开启远端润色一致。`results/` 与 `results_fresh/` 下的清单、缓存与改动排名含历史原文，已在 `.gitignore` 中排除。报告只摘录评估需要的短句，不提交完整历史。

## 已知限制

- 没有录音，不能给出端到端 ASR 修正率。
- 目标术语在历史里的真实错例很少，定向集主要靠合成变体。
- 只测一个远端模型与一个 `heavy` 档，不外推到其他模型。
- 200 条术语表比 10 条覆盖更多短语，真正不含任何术语的硬负例会减少。
