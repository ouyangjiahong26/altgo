//! 转写文本历史（持久化 JSON，不含录音）。
//!
//! 历史通过 `HistoryStore` 访问：调用方不直接处理文件路径，
//! 也不调用模块私有 helper。所有路径 I/O 与并发互斥都由 store 内部完成。

use crate::error::HistoryError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use uuid::Uuid;

static HISTORY_IO_LOCK: Mutex<()> = Mutex::new(());

/// 历史条目的耗时与来源元数据。整体挂在 `HistoryEntry::meta` 的 `Option` 下：
/// 旧版本写入的条目缺该键，`#[serde(default)]` 补 `None`。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryMeta {
    /// 转写后端：`"local"`（本地）或 `"online"`（在线）。
    pub backend: String,
    /// 录音时长（毫秒）。
    pub recording_ms: u64,
    /// 转写耗时（毫秒）。
    pub transcribe_ms: u64,
    /// 润色耗时（毫秒），未启用润色时为 `None`。
    pub polish_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub created_at_ms: u64,
    /// 原始转写（快捷润色以此为输入）
    pub raw_text: String,
    /// 当前展示文本（润色后或与 raw 相同）
    pub text: String,
    /// 本次转写的耗时与后端来源，旧条目无此数据。
    #[serde(default)]
    pub meta: Option<HistoryMeta>,
}

#[derive(Serialize, Deserialize, Default)]
struct HistoryFile {
    entries: Vec<HistoryEntry>,
}

fn load_raw(path: &std::path::Path) -> Result<HistoryFile, HistoryError> {
    if !path.exists() {
        return Ok(HistoryFile::default());
    }
    let s = fs::read_to_string(path)?;
    if s.trim().is_empty() {
        return Ok(HistoryFile::default());
    }
    serde_json::from_str(&s)
        .map_err(|e| HistoryError::JsonError(format!("parse history json: {e}")))
}

fn save_raw(path: &std::path::Path, data: &HistoryFile) -> Result<(), HistoryError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let s = serde_json::to_string_pretty(data)
        .map_err(|e| HistoryError::SerializeError(format!("serialize history: {e}")))?;
    fs::write(path, s)?;

    // 将文件权限限制为仅属主可读写（保护落盘的转写数据）。
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }

    Ok(())
}

/// 持有历史文件路径并提供具名操作。
/// 调用方从不直接接触路径，所有 I/O 一律经由 store 完成。
#[derive(Clone)]
pub struct HistoryStore {
    path: std::path::PathBuf,
}

impl HistoryStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn list(&self) -> Result<Vec<HistoryEntry>, HistoryError> {
        let _g = HISTORY_IO_LOCK
            .lock()
            .map_err(|_| HistoryError::LockPoisoned)?;
        let file = load_raw(&self.path)?;
        Ok(file.entries)
    }

    /// 当前存储的条目数量。
    pub fn count(&self) -> Result<usize, HistoryError> {
        Ok(self.list()?.len())
    }

    pub fn append(
        &self,
        raw_text: String,
        text: String,
        meta: HistoryMeta,
    ) -> Result<HistoryEntry, HistoryError> {
        let _g = HISTORY_IO_LOCK
            .lock()
            .map_err(|_| HistoryError::LockPoisoned)?;
        let mut file = load_raw(&self.path)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let entry = HistoryEntry {
            id: Uuid::new_v4().to_string(),
            created_at_ms: now,
            raw_text,
            text,
            meta: Some(meta),
        };
        file.entries.insert(0, entry.clone());
        save_raw(&self.path, &file)?;
        Ok(entry)
    }

    pub fn delete(&self, ids: &[String]) -> Result<usize, HistoryError> {
        let _g = HISTORY_IO_LOCK
            .lock()
            .map_err(|_| HistoryError::LockPoisoned)?;
        let mut file = load_raw(&self.path)?;
        let before = file.entries.len();
        let id_set: std::collections::HashSet<&str> = ids.iter().map(|s| s.as_str()).collect();
        file.entries.retain(|e| !id_set.contains(e.id.as_str()));
        save_raw(&self.path, &file)?;
        Ok(before - file.entries.len())
    }

    pub fn clear(&self) -> Result<(), HistoryError> {
        let _g = HISTORY_IO_LOCK
            .lock()
            .map_err(|_| HistoryError::LockPoisoned)?;
        save_raw(&self.path, &HistoryFile::default())
    }

    pub fn get(&self, id: &str) -> Result<Option<HistoryEntry>, HistoryError> {
        let _g = HISTORY_IO_LOCK
            .lock()
            .map_err(|_| HistoryError::LockPoisoned)?;
        let file = load_raw(&self.path)?;
        Ok(file.entries.iter().find(|e| e.id == id).cloned())
    }

    pub fn update_text(&self, id: &str, new_text: String) -> Result<HistoryEntry, HistoryError> {
        let _g = HISTORY_IO_LOCK
            .lock()
            .map_err(|_| HistoryError::LockPoisoned)?;
        let mut file = load_raw(&self.path)?;
        for e in &mut file.entries {
            if e.id == id {
                e.text = new_text;
                let out = e.clone();
                save_raw(&self.path, &file)?;
                return Ok(out);
            }
        }
        Err(HistoryError::NotFound(id.to_string()))
    }

    /// 用润色后文本更新条目。先查存在，再写入。
    pub fn polish_entry(&self, id: &str, new_text: &str) -> Result<HistoryEntry, HistoryError> {
        let _entry = self
            .get(id)?
            .ok_or_else(|| HistoryError::NotFound(id.to_string()))?;
        self.update_text(id, new_text.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn make_store() -> (tempfile::TempDir, HistoryStore) {
        let dir = tempdir().unwrap();
        let store = HistoryStore::new(dir.path().join("history.json"));
        (dir, store)
    }

    fn meta() -> HistoryMeta {
        HistoryMeta {
            backend: "local".to_string(),
            recording_ms: 1200,
            transcribe_ms: 340,
            polish_ms: Some(560),
        }
    }

    #[test]
    fn append_and_list() {
        let (_dir, store) = make_store();
        let e1 = store
            .append("raw one".into(), "one".into(), meta())
            .unwrap();
        let e2 = store
            .append("raw two".into(), "two".into(), meta())
            .unwrap();
        let list = store.list().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, e2.id);
        assert_eq!(list[1].id, e1.id);
    }

    #[test]
    fn append_persists_meta() {
        let (_dir, store) = make_store();
        store
            .append(
                "raw".into(),
                "text".into(),
                HistoryMeta {
                    backend: "online".to_string(),
                    recording_ms: 2500,
                    transcribe_ms: 1800,
                    polish_ms: None,
                },
            )
            .unwrap();
        let e = store.list().unwrap().remove(0);
        let m = e.meta.expect("meta persisted");
        assert_eq!(m.backend, "online");
        assert_eq!(m.recording_ms, 2500);
        assert_eq!(m.transcribe_ms, 1800);
        assert_eq!(m.polish_ms, None);
    }

    #[test]
    fn legacy_entries_without_meta_still_parse() {
        // 旧版本写入的条目没有 meta 键，读取时补 None 而不是解析失败。
        let dir = tempdir().unwrap();
        let path = dir.path().join("history.json");
        std::fs::write(
            &path,
            r#"{"entries":[{"id":"a","createdAtMs":1,"rawText":"r","text":"t"}]}"#,
        )
        .unwrap();
        let store = HistoryStore::new(path);
        let e = store.list().unwrap().remove(0);
        assert_eq!(e.meta, None);
    }

    #[test]
    fn delete_and_clear() {
        let (_dir, store) = make_store();
        let e = store.append("r".into(), "t".into(), meta()).unwrap();
        store.delete(std::slice::from_ref(&e.id)).unwrap();
        assert!(store.list().unwrap().is_empty());
        store.append("a".into(), "b".into(), meta()).unwrap();
        store.clear().unwrap();
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn update_text() {
        let (_dir, store) = make_store();
        let e = store.append("raw".into(), "old".into(), meta()).unwrap();
        let updated = store.update_text(&e.id, "new".into()).unwrap();
        assert_eq!(updated.text, "new");
        assert_eq!(updated.raw_text, "raw");
    }

    #[test]
    fn count_starts_at_zero() {
        let (_dir, store) = make_store();
        assert_eq!(store.count().unwrap(), 0);
    }

    #[test]
    fn count_reflects_appends_and_deletes() {
        let (_dir, store) = make_store();
        assert_eq!(store.count().unwrap(), 0);
        let e1 = store.append("a".into(), "a".into(), meta()).unwrap();
        let e2 = store.append("b".into(), "b".into(), meta()).unwrap();
        assert_eq!(store.count().unwrap(), 2);
        store.delete(&[e1.id]).unwrap();
        assert_eq!(store.count().unwrap(), 1);
        store.clear().unwrap();
        assert_eq!(store.count().unwrap(), 0);
        let _ = e2;
    }

    #[test]
    fn get_returns_entry_by_id() {
        let (_dir, store) = make_store();
        let e = store.append("raw".into(), "text".into(), meta()).unwrap();
        let fetched = store.get(&e.id).unwrap();
        assert_eq!(fetched, Some(e));
    }

    #[test]
    fn get_returns_none_for_missing_id() {
        let (_dir, store) = make_store();
        assert!(store.get("nonexistent").unwrap().is_none());
    }

    #[test]
    fn polish_entry_updates_text() {
        let (_dir, store) = make_store();
        let e = store
            .append("raw text".into(), "old text".into(), meta())
            .unwrap();
        let updated = store.polish_entry(&e.id, "polished text").unwrap();
        assert_eq!(updated.text, "polished text");
        assert_eq!(updated.raw_text, "raw text");
        // 再次读取确认持久化
        let fetched = store.get(&e.id).unwrap().unwrap();
        assert_eq!(fetched.text, "polished text");
    }

    #[test]
    fn polish_entry_fails_for_missing_id() {
        let (_dir, store) = make_store();
        let err = store.polish_entry("nonexistent", "text").unwrap_err();
        assert!(err.to_string().contains("not found"));
    }
}
