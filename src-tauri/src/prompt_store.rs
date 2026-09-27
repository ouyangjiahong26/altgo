//! Prompt 模板管理。
//!
//! 从 `resources/prompts/` 加载 prompt 模板：
//! - `base.txt`：共用指令 + 中文写作要求
//! - `{level}-suffix.txt`：各档位专属指令
//!
//! 运行时组合：`base.txt` + `{level}-suffix.txt` → 完整 system prompt。
//! 模板启动时加载一次；修改文件需重启应用生效。

use crate::polisher::PolishLevel;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Prompt 加载或校验过程中可能出现的错误。
#[derive(Debug, thiserror::Error)]
pub enum PromptError {
    #[error("Prompt file not found: {0}")]
    FileNotFound(PathBuf),

    #[error("Prompt file is empty: {0}")]
    EmptyFile(PathBuf),

    #[error("Failed to read prompt file {path}: {source}")]
    ReadError {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// 管理 prompt 模板。
#[derive(Clone, Debug)]
pub struct PromptStore {
    prompts_dir: PathBuf,
    cache: Arc<Mutex<PromptCache>>,
}

struct PromptCache {
    base: String,
    suffixes: HashMap<PolishLevel, String>,
}

impl std::fmt::Debug for PromptCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PromptCache")
            .field("base_len", &self.base.len())
            .field("suffixes_count", &self.suffixes.len())
            .finish()
    }
}

impl PromptStore {
    /// 为给定 prompts 目录创建新的 PromptStore。
    ///
    /// 不会立即加载 prompt——调用 `load()` 或 `ensure_loaded()` 加载。
    pub fn new(prompts_dir: PathBuf) -> Self {
        Self {
            prompts_dir,
            cache: Arc::new(Mutex::new(PromptCache {
                base: String::new(),
                suffixes: HashMap::new(),
            })),
        }
    }

    /// 从磁盘加载全部 prompt 模板。
    ///
    /// 任一文件缺失或为空时返回错误。
    pub fn load(&self) -> Result<(), PromptError> {
        let base = self.load_file("base.txt")?;
        let light = self.load_file("light-suffix.txt")?;
        let medium = self.load_file("medium-suffix.txt")?;
        let heavy = self.load_file("heavy-suffix.txt")?;

        let mut cache = self.cache.lock().unwrap();
        cache.base = base;
        cache.suffixes.insert(PolishLevel::Light, light);
        cache.suffixes.insert(PolishLevel::Medium, medium);
        cache.suffixes.insert(PolishLevel::Heavy, heavy);

        Ok(())
    }

    /// 缓存为空时从磁盘加载，否则返回已缓存的 prompt。
    pub fn ensure_loaded(&self) -> Result<(), PromptError> {
        let cache = self.cache.lock().unwrap();
        if cache.base.is_empty() {
            drop(cache);
            self.load()
        } else {
            Ok(())
        }
    }

    /// 组合出指定润色档位的完整 system prompt。
    ///
    /// 未加载时返回错误。
    pub fn get_system_prompt(&self, level: PolishLevel) -> Result<String, PromptError> {
        if matches!(level, PolishLevel::None) {
            return Ok(String::new());
        }

        let cache = self.cache.lock().unwrap();
        if cache.base.is_empty() {
            return Err(PromptError::FileNotFound(self.prompts_dir.join("base.txt")));
        }

        let suffix = cache.suffixes.get(&level).ok_or_else(|| {
            PromptError::FileNotFound(
                self.prompts_dir
                    .join(format!("{:?}-suffix.txt", level).to_lowercase()),
            )
        })?;

        Ok(format!("{}\n\n{}", cache.base.trim(), suffix.trim()))
    }

    fn load_file(&self, filename: &str) -> Result<String, PromptError> {
        let path = self.prompts_dir.join(filename);

        if !path.exists() {
            return Err(PromptError::FileNotFound(path));
        }

        let content = fs::read_to_string(&path).map_err(|e| PromptError::ReadError {
            path: path.clone(),
            source: e,
        })?;

        if content.trim().is_empty() {
            return Err(PromptError::EmptyFile(path));
        }

        Ok(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    fn create_test_prompts(dir: &Path) {
        fs::write(dir.join("base.txt"), "Base prompt").unwrap();
        fs::write(dir.join("light-suffix.txt"), "Light suffix").unwrap();
        fs::write(dir.join("medium-suffix.txt"), "Medium suffix").unwrap();
        fs::write(dir.join("heavy-suffix.txt"), "Heavy suffix").unwrap();
    }

    #[test]
    fn test_load_success() {
        let temp_dir = TempDir::new().unwrap();
        create_test_prompts(temp_dir.path());

        let store = PromptStore::new(temp_dir.path().to_path_buf());
        assert!(store.load().is_ok());
    }

    #[test]
    fn test_load_missing_file() {
        let temp_dir = TempDir::new().unwrap();
        let store = PromptStore::new(temp_dir.path().to_path_buf());

        let result = store.load();
        assert!(matches!(result, Err(PromptError::FileNotFound(_))));
    }

    #[test]
    fn test_load_empty_file() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("base.txt"), "").unwrap();

        let store = PromptStore::new(temp_dir.path().to_path_buf());
        let result = store.load();
        assert!(matches!(result, Err(PromptError::EmptyFile(_))));
    }

    #[test]
    fn test_get_system_prompt_composition() {
        let temp_dir = TempDir::new().unwrap();
        create_test_prompts(temp_dir.path());

        let store = PromptStore::new(temp_dir.path().to_path_buf());
        store.load().unwrap();

        let prompt = store.get_system_prompt(PolishLevel::Light).unwrap();
        assert!(prompt.contains("Base prompt"));
        assert!(prompt.contains("Light suffix"));
    }

    #[test]
    fn test_get_system_prompt_none_level() {
        let temp_dir = TempDir::new().unwrap();
        let store = PromptStore::new(temp_dir.path().to_path_buf());

        let prompt = store.get_system_prompt(PolishLevel::None).unwrap();
        assert_eq!(prompt, "");
    }

    #[test]
    fn test_ensure_loaded_lazy() {
        let temp_dir = TempDir::new().unwrap();
        create_test_prompts(temp_dir.path());

        let store = PromptStore::new(temp_dir.path().to_path_buf());

        // 首次调用触发加载
        assert!(store.ensure_loaded().is_ok());

        // 第二次调用命中缓存
        assert!(store.ensure_loaded().is_ok());
    }
}
