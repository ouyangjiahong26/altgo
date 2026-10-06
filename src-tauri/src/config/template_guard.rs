//! configs/altgo.toml 模板与配置模型的一致性守护。

use super::*;

/// 模板的字段清单必须与配置模型的标准字段名一致，防止模板再次漂移成旧名或漏掉新字段。
#[test]
fn test_repo_template_matches_config_fields() {
    let template = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("configs")
        .join("altgo.toml");
    let text = std::fs::read_to_string(&template).expect("模板文件应存在于仓库内");
    let value: toml::Value = toml::from_str(&text).expect("模板应是合法 TOML");

    let mut keys = Vec::new();
    collect_key_paths(&value, "", &mut keys);
    keys.sort();
    let mut expected = vec![
        "gui.auto_check_update",
        "gui.language",
        "gui.overlay_position",
        "key_listener.double_click_interval",
        "key_listener.key_name",
        "key_listener.long_press_threshold",
        "key_listener.min_press_duration",
        "output.inject_text",
        "output.prefer_polished",
        "polisher.api_base_url",
        "polisher.api_key",
        "polisher.level",
        "polisher.max_tokens",
        "polisher.model",
        "polisher.protocol",
        "polisher.system_prompt",
        "polisher.temperature",
        "polisher.thinking_level",
        "polisher.timeout",
        "recorder.sample_rate",
        "transcriber.backend",
        "transcriber.language",
        "transcriber.model",
        "transcriber.online.api_base_url",
        "transcriber.online.api_key",
        "transcriber.online.model",
        "transcriber.online.timeout",
        "transcriber.threads",
    ];
    expected.sort();
    assert_eq!(
        keys, expected,
        "configs/altgo.toml 的字段清单与 config.rs 不一致"
    );

    // 模板必须能直接反序列化成配置，注释掉的可选项不参与。
    Config::load(&template).expect("模板应能被 Config::load 解析");
}

/// 递归收集 TOML 树的叶节点键路径，形如 `polisher.timeout`。
fn collect_key_paths(value: &toml::Value, prefix: &str, out: &mut Vec<String>) {
    let table = match value {
        toml::Value::Table(t) => t,
        _ => {
            out.push(prefix.to_string());
            return;
        }
    };
    for (key, child) in table {
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        collect_key_paths(child, &path, out);
    }
}
