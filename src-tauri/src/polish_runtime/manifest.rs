//! 润色运行时清单：固定构建号、各平台产物与保留规则。
//!
//! 构建号写死在代码里，产物大小与 SHA-256 一并固定，因此下载与手动放置都能被验证。
//! 保留规则只按文件名判定：服务器本体加全部动态库，另外保留许可证文本。其余命令行
//! 工具（`llama-cli`、`llama-bench` 等二十来个）不落地，实测剔除它们不影响服务启动。

use std::borrow::Cow;
use std::path::PathBuf;

/// 固定的 llama.cpp 构建号。升级时同时更新下面的产物大小与 SHA-256。
pub const RUNTIME_BUILD: &str = "b11612";

/// 运行时目录的相对层级：`<配置目录>/altgo/runtime/llama-server/<构建号>`。
const RUNTIME_DIR_SEGMENTS: [&str; 3] = ["altgo", "runtime", "llama-server"];

/// 一个平台的运行时产物。
pub struct RuntimeSpec {
    /// 目标三元组标识，用于日志与错误文案。
    pub target: &'static str,
    /// 官方产物文件名。
    pub archive_name: &'static str,
    pub archive_size: u64,
    /// 官方产物的 SHA-256，用于识别中断下载与损坏缓存。
    pub archive_sha256: Cow<'static, str>,
    /// 压缩包内的顶层目录，平铺的 zip 为空串。
    pub archive_root: &'static str,
    /// 运行时可执行文件名。
    pub executable: &'static str,
    /// 共享库的文件名后缀（Windows 是 `.dll`，Linux 是 `.so`）。
    pub shared_lib_suffix: &'static str,
    /// 结构检查必须存在的核心库。
    pub required_libraries: &'static [&'static str],
    /// CPU 后端库的文件名前缀，至少存在一个才算完整。
    pub cpu_lib_prefix: &'static str,
}

impl RuntimeSpec {
    /// 该文件是否要保留到运行时目录。
    pub fn keeps(&self, name: &str) -> bool {
        name == self.executable
            || is_shared_library(name, self.shared_lib_suffix)
            || name.starts_with("LICENSE")
    }
}

/// 共享库判定：`libllama.so` 与 `libllama.so.0.6.0` 都算，`libfoo.so.bak` 不算。
///
/// Linux 归档里真实文件带版本后缀，不带版本的名字只是指向它的符号链接，两者都要保留，
/// 否则解包后只剩悬空的链接。
fn is_shared_library(name: &str, suffix: &str) -> bool {
    let Some((_, rest)) = name.rsplit_once(suffix) else {
        return false;
    };
    rest.is_empty()
        || (rest.starts_with('.') && rest.chars().all(|c| c == '.' || c.is_ascii_digit()))
}

const WINDOWS_LIBRARIES: &[&str] = &[
    "llama-server-impl.dll",
    "llama-common.dll",
    "llama.dll",
    "ggml.dll",
    "ggml-base.dll",
];

const LINUX_LIBRARIES: &[&str] = &[
    "libllama-server-impl.so",
    "libllama-common.so",
    "libllama.so",
    "libggml.so",
    "libggml-base.so",
];

/// Windows x64 CPU 产物。解包实测 51 个文件，保留后 30 个，约 46.7 MiB。
static WINDOWS_X64: RuntimeSpec = RuntimeSpec {
    target: "x86_64-pc-windows-msvc",
    archive_name: "llama-b11612-bin-win-cpu-x64.zip",
    archive_size: 19_553_433,
    archive_sha256: Cow::Borrowed(
        "b8456ccb1be98365a5a2227efa5c495508a6a3ebce881c6189a1dbeaaa170b69",
    ),
    archive_root: "",
    executable: "llama-server.exe",
    shared_lib_suffix: ".dll",
    required_libraries: WINDOWS_LIBRARIES,
    cpu_lib_prefix: "ggml-cpu",
};

/// Windows arm64 CPU 产物。解包实测 38 个文件，只有一份 CPU 后端库。
static WINDOWS_ARM64: RuntimeSpec = RuntimeSpec {
    target: "aarch64-pc-windows-msvc",
    archive_name: "llama-b11612-bin-win-cpu-arm64.zip",
    archive_size: 12_420_761,
    archive_sha256: Cow::Borrowed(
        "56374f4c3b3678005cd84278fa9c3e67f7a318e404071aeca6e498bbc1d4086c",
    ),
    archive_root: "",
    executable: "llama-server.exe",
    shared_lib_suffix: ".dll",
    required_libraries: WINDOWS_LIBRARIES,
    cpu_lib_prefix: "ggml-cpu",
};

/// Linux x64 产物。压缩包有顶层目录 `llama-b11612/`，共享库以符号链接暴露 soname。
static LINUX_X64: RuntimeSpec = RuntimeSpec {
    target: "x86_64-unknown-linux-gnu",
    archive_name: "llama-b11612-bin-ubuntu-x64.tar.gz",
    archive_size: 17_774_766,
    archive_sha256: Cow::Borrowed(
        "06fbaabe471aab27727882cd8f1b5c231d2eaa346fce1a3171041e2868f08c2a",
    ),
    archive_root: "llama-b11612",
    executable: "llama-server",
    shared_lib_suffix: ".so",
    required_libraries: LINUX_LIBRARIES,
    cpu_lib_prefix: "libggml-cpu",
};

/// Linux arm64 产物。
static LINUX_ARM64: RuntimeSpec = RuntimeSpec {
    target: "aarch64-unknown-linux-gnu",
    archive_name: "llama-b11612-bin-ubuntu-arm64.tar.gz",
    archive_size: 13_867_985,
    archive_sha256: Cow::Borrowed(
        "706f3aefe954ce897cb515493a3940192c1197075cb4f7872159e55b3dff9cb7",
    ),
    archive_root: "llama-b11612",
    executable: "llama-server",
    shared_lib_suffix: ".so",
    required_libraries: LINUX_LIBRARIES,
    cpu_lib_prefix: "libggml-cpu",
};

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
pub static CURRENT_SPEC: Option<&RuntimeSpec> = Some(&WINDOWS_X64);
#[cfg(all(target_os = "windows", target_arch = "aarch64"))]
pub static CURRENT_SPEC: Option<&RuntimeSpec> = Some(&WINDOWS_ARM64);
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub static CURRENT_SPEC: Option<&RuntimeSpec> = Some(&LINUX_X64);
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
pub static CURRENT_SPEC: Option<&RuntimeSpec> = Some(&LINUX_ARM64);
#[cfg(not(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "windows", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "aarch64"),
)))]
pub static CURRENT_SPEC: Option<&RuntimeSpec> = None;

/// 当前平台的运行时产物，不支持的平台返回 `None`。
pub fn current_spec() -> Option<&'static RuntimeSpec> {
    CURRENT_SPEC
}

/// 运行时目录：配置目录下的 `altgo/runtime/llama-server/<构建号>`。
pub fn runtime_dir() -> PathBuf {
    let mut dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    for segment in RUNTIME_DIR_SEGMENTS {
        dir = dir.join(segment);
    }
    dir.join(RUNTIME_BUILD)
}

/// 清单里的全部平台产物，用于一致性测试。
pub static ALL_SPECS: [&RuntimeSpec; 4] = [&WINDOWS_X64, &WINDOWS_ARM64, &LINUX_X64, &LINUX_ARM64];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_current_spec_exists_on_supported_targets() {
        let spec = current_spec();
        if cfg!(any(
            all(target_os = "windows", target_arch = "x86_64"),
            all(target_os = "windows", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64"),
            all(target_os = "linux", target_arch = "aarch64"),
        )) {
            let spec = spec.expect("受支持的平台必须有清单条目");
            assert!(spec.archive_name.contains(RUNTIME_BUILD));
            assert!(spec.executable.starts_with("llama-server"));
        }
    }

    #[test]
    fn test_all_specs_are_pinned_and_distinct() {
        let mut archive_names = HashSet::new();
        for spec in ALL_SPECS {
            assert!(archive_names.insert(spec.archive_name), "产物名重复");
            assert!(spec.archive_size > 1_000_000, "{}", spec.archive_name);
            assert_eq!(spec.archive_sha256.len(), 64, "{}", spec.archive_name);
            assert!(
                spec.archive_sha256
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
                "{}",
                spec.archive_name
            );
            assert!(!spec.required_libraries.is_empty());
            assert!(spec.shared_lib_suffix.starts_with('.'));
            assert!(!spec.cpu_lib_prefix.is_empty());
        }
    }

    #[test]
    fn test_keeps_rule_selects_server_and_libraries_only() {
        let spec = &WINDOWS_X64;
        assert!(spec.keeps("llama-server.exe"));
        assert!(spec.keeps("llama-server-impl.dll"));
        assert!(spec.keeps("ggml-cpu-alderlake.dll"));
        assert!(spec.keeps("LICENSE-LLVM-OpenMP"));
        assert!(!spec.keeps("llama-cli.exe"));
        assert!(!spec.keeps("llama-bench-impl.dll.removed"));

        let linux = &LINUX_X64;
        assert!(linux.keeps("llama-server"));
        assert!(linux.keeps("libllama.so"));
        assert!(linux.keeps("libggml-cpu-zen4.so"));
        assert!(!linux.keeps("llama-cli"));
        assert!(!linux.keeps("ggml-rpc-server"));
        // 真实归档里带版本后缀的是真身，不带版本的名字只是指向它的符号链接，两者都要保留。
        assert!(linux.keeps("libllama.so.0.6.0"));
        assert!(linux.keeps("libllama-common.so.0"));
        assert!(linux.keeps("libggml-base.so.0.26.0"));
        assert!(!linux.keeps("libfoo.so.bak"));
        assert!(!linux.keeps("libfoo.socket"));
    }

    #[test]
    fn test_linux_archives_share_one_root_directory() {
        assert_eq!(LINUX_X64.archive_root, "llama-b11612");
        assert_eq!(LINUX_ARM64.archive_root, LINUX_X64.archive_root);
        assert!(WINDOWS_X64.archive_root.is_empty());
        assert!(WINDOWS_ARM64.archive_root.is_empty());
    }

    #[test]
    fn test_runtime_dir_is_build_scoped() {
        let dir = runtime_dir();
        assert!(dir.ends_with(RUNTIME_BUILD));
        assert!(dir.to_string_lossy().contains("llama-server"));
    }
}
