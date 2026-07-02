use anyhow::Result;
use async_trait::async_trait;
use aurora_executor_api::{ExecutionInput, ExecutionOutput, Executor};
use extism::{Manifest, Plugin, Wasm};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Wall-clock cap for a single plugin call: a hostile or buggy `.wasm` must not
/// be able to hang the run indefinitely.
const PLUGIN_TIMEOUT: Duration = Duration::from_secs(300);

/// Memory cap for a plugin (WASM pages are 64 KiB each), so a runaway module
/// cannot exhaust host memory. 8192 pages = 512 MiB.
const PLUGIN_MAX_MEMORY_PAGES: u32 = 8192;

pub struct WasmExecutor {
    name: String,
    plugin_path: PathBuf,
}

impl WasmExecutor {
    pub fn load(name: String, path: PathBuf) -> Result<Self> {
        if !path.exists() {
            anyhow::bail!("Plugin not found: {:?}", path);
        }
        Ok(WasmExecutor {
            name,
            plugin_path: path,
        })
    }
}

#[async_trait]
impl Executor for WasmExecutor {
    fn name(&self) -> &str {
        &self.name
    }

    async fn execute(&self, input: ExecutionInput) -> Result<ExecutionOutput> {
        let plugin_path = self.plugin_path.clone();
        let input_json = serde_json::to_vec(&input)?;

        tokio::task::spawn_blocking(move || -> Result<ExecutionOutput> {
            let wasm = Wasm::file(&plugin_path);
            // Plugins are untrusted, unsigned code: bound their time and memory,
            // and grant no host access. WASI stays disabled (the `false` below),
            // so a plugin has no filesystem or network by default.
            let manifest = Manifest::new([wasm])
                .with_timeout(PLUGIN_TIMEOUT)
                .with_memory_max(PLUGIN_MAX_MEMORY_PAGES)
                .disallow_all_hosts();
            let mut plugin = Plugin::new(&manifest, [], false)?;
            let output_bytes = plugin.call::<&[u8], &[u8]>("execute", &input_json)?;
            Ok(serde_json::from_slice(output_bytes)?)
        })
        .await?
    }
}

pub fn discover_plugins() -> Vec<(String, PathBuf)> {
    let plugins_dir = dirs::home_dir()
        .map(|h| h.join(".aurora/plugins"))
        .unwrap_or_default();
    discover_plugins_in(&plugins_dir)
}

/// Lists `*.wasm` files in `dir` as `(name, path)` pairs, where `name` is the
/// file stem. A missing directory yields an empty list. Split from
/// [`discover_plugins`] so the discovery logic is testable without a real home
/// directory.
pub fn discover_plugins_in(dir: &Path) -> Vec<(String, PathBuf)> {
    if !dir.exists() {
        return vec![];
    }

    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension()?.to_str()? == "wasm" {
                let name = path.file_stem()?.to_string_lossy().to_string();
                Some((name, path))
            } else {
                None
            }
        })
        .collect()
}

/// Registers each discovered plugin into `executors`. A native (built-in)
/// executor always wins: a plugin whose name is already taken is skipped with
/// an stderr warning, and so is a plugin that fails to load. Returns the names
/// actually registered.
pub fn register_plugins(
    executors: &mut HashMap<String, Arc<dyn Executor>>,
    discovered: Vec<(String, PathBuf)>,
) -> Vec<String> {
    let mut registered = Vec::new();
    for (name, path) in discovered {
        if executors.contains_key(&name) {
            eprintln!(
                "aurora: ignoring plugin '{}' ({}): a built-in executor already uses that name",
                name,
                path.display()
            );
            continue;
        }
        match WasmExecutor::load(name.clone(), path.clone()) {
            Ok(executor) => {
                executors.insert(name.clone(), Arc::new(executor) as Arc<dyn Executor>);
                registered.push(name);
            }
            Err(e) => eprintln!(
                "aurora: skipping plugin '{}' ({}): {}",
                name,
                path.display(),
                e
            ),
        }
    }
    registered
}
