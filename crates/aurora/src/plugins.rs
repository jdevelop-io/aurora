use anyhow::Result;
use async_trait::async_trait;
use aurora_executor_api::{ExecutionInput, ExecutionOutput, Executor};
use extism::{Manifest, Plugin, Wasm};
use std::path::PathBuf;

pub struct WasmExecutor {
    name: String,
    plugin_path: PathBuf,
}

impl WasmExecutor {
    pub fn load(name: String, path: PathBuf) -> Result<Self> {
        if !path.exists() { anyhow::bail!("Plugin not found: {:?}", path); }
        Ok(WasmExecutor { name, plugin_path: path })
    }
}

#[async_trait]
impl Executor for WasmExecutor {
    fn name(&self) -> &str { &self.name }

    async fn execute(&self, input: ExecutionInput) -> Result<ExecutionOutput> {
        let plugin_path = self.plugin_path.clone();
        let input_json = serde_json::to_vec(&input)?;

        tokio::task::spawn_blocking(move || -> Result<ExecutionOutput> {
            let wasm = Wasm::file(&plugin_path);
            let manifest = Manifest::new([wasm]);
            let mut plugin = Plugin::new(&manifest, [], false)?;
            let output_bytes = plugin.call::<&[u8], &[u8]>("execute", &input_json)?;
            Ok(serde_json::from_slice(output_bytes)?)
        }).await?
    }
}

pub fn discover_plugins() -> Vec<(String, PathBuf)> {
    let plugins_dir = dirs::home_dir()
        .map(|h| h.join(".aurora/plugins"))
        .unwrap_or_default();

    if !plugins_dir.exists() { return vec![]; }

    std::fs::read_dir(&plugins_dir)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension()?.to_str()? == "wasm" {
                let name = path.file_stem()?.to_string_lossy().to_string();
                Some((name, path))
            } else { None }
        })
        .collect()
}
