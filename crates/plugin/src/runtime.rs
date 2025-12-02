//! WASM plugin runtime using wasmtime.
//!
//! This module provides the core runtime for loading and executing
//! WebAssembly plugins.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use wasmtime::{Caller, Engine, Linker, Memory, Module, Store, TypedFunc};

use crate::error::{PluginError, Result};
use crate::host::PluginState;
use crate::manifest::PluginManifest;

/// Store data for the WASM runtime.
pub struct StoreData {
    /// Plugin state accessible via host functions.
    pub state: PluginState,
    /// Memory exported by the plugin (for string passing).
    pub memory: Option<Memory>,
}

impl StoreData {
    fn new(state: PluginState) -> Self {
        Self {
            state,
            memory: None,
        }
    }
}

/// A loaded WASM plugin.
pub struct Plugin {
    /// Plugin manifest.
    manifest: PluginManifest,
    /// Plugin directory.
    plugin_dir: PathBuf,
    /// Compiled WASM module.
    module: Module,
}

impl Plugin {
    /// Gets the plugin name.
    pub fn name(&self) -> &str {
        &self.manifest.plugin.name
    }

    /// Gets the plugin version.
    pub fn version(&self) -> &str {
        &self.manifest.plugin.version
    }

    /// Gets the plugin manifest.
    pub fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    /// Gets the plugin directory.
    pub fn plugin_dir(&self) -> &Path {
        &self.plugin_dir
    }
}

/// Runtime for executing WASM plugins.
pub struct PluginRuntime {
    /// The wasmtime engine.
    engine: Engine,
    /// Loaded plugins.
    plugins: HashMap<String, Arc<Plugin>>,
}

impl Default for PluginRuntime {
    fn default() -> Self {
        Self::new().expect("Failed to create default plugin runtime")
    }
}

impl PluginRuntime {
    /// Creates a new plugin runtime.
    pub fn new() -> Result<Self> {
        let engine = Engine::default();
        Ok(Self {
            engine,
            plugins: HashMap::new(),
        })
    }

    /// Creates a plugin runtime with custom engine configuration.
    pub fn with_engine(engine: Engine) -> Self {
        Self {
            engine,
            plugins: HashMap::new(),
        }
    }

    /// Gets the wasmtime engine.
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Gets a loaded plugin by name.
    pub fn get_plugin(&self, name: &str) -> Option<Arc<Plugin>> {
        self.plugins.get(name).cloned()
    }

    /// Gets all loaded plugins.
    pub fn plugins(&self) -> impl Iterator<Item = &Arc<Plugin>> {
        self.plugins.values()
    }

    /// Loads a plugin from a directory containing a manifest.
    ///
    /// The directory should contain:
    /// - `plugin.json` - Plugin manifest
    /// - `plugin.wasm` - WASM module (or custom path in manifest)
    pub fn load_plugin(&mut self, plugin_dir: &Path) -> Result<Arc<Plugin>> {
        let manifest_path = plugin_dir.join("plugin.json");

        if !manifest_path.exists() {
            return Err(PluginError::NotFound(manifest_path));
        }

        let manifest = PluginManifest::from_file(&manifest_path)?;
        let wasm_path = plugin_dir.join(&manifest.plugin.wasm);

        if !wasm_path.exists() {
            return Err(PluginError::NotFound(wasm_path));
        }

        let wasm_bytes = std::fs::read(&wasm_path)?;
        let module = Module::new(&self.engine, &wasm_bytes)?;

        let plugin = Arc::new(Plugin {
            manifest,
            plugin_dir: plugin_dir.to_path_buf(),
            module,
        });

        let name = plugin.name().to_string();
        self.plugins.insert(name.clone(), plugin.clone());

        Ok(plugin)
    }

    /// Loads a plugin directly from WASM bytes.
    pub fn load_plugin_from_bytes(
        &mut self,
        name: &str,
        version: &str,
        wasm_bytes: &[u8],
    ) -> Result<Arc<Plugin>> {
        let manifest = PluginManifest::minimal(name, version);
        let module = Module::new(&self.engine, wasm_bytes)?;

        let plugin = Arc::new(Plugin {
            manifest,
            plugin_dir: PathBuf::new(),
            module,
        });

        self.plugins.insert(name.to_string(), plugin.clone());
        Ok(plugin)
    }

    /// Unloads a plugin by name.
    pub fn unload_plugin(&mut self, name: &str) -> bool {
        self.plugins.remove(name).is_some()
    }

    /// Creates a new plugin instance for execution.
    pub fn create_instance(&self, plugin: &Plugin) -> Result<PluginInstance> {
        PluginInstance::new(&self.engine, plugin)
    }

    /// Creates a new plugin instance with custom state.
    pub fn create_instance_with_state(
        &self,
        plugin: &Plugin,
        state: PluginState,
    ) -> Result<PluginInstance> {
        PluginInstance::with_state(&self.engine, plugin, state)
    }
}

/// An instantiated plugin ready for execution.
pub struct PluginInstance {
    store: Store<StoreData>,
    /// Cached function: plugin_name() -> ptr, len
    fn_plugin_name: Option<TypedFunc<(), (i32, i32)>>,
    /// Cached function: plugin_version() -> ptr, len
    fn_plugin_version: Option<TypedFunc<(), (i32, i32)>>,
    /// Cached function: on_beam_start(ptr, len)
    fn_on_beam_start: Option<TypedFunc<(i32, i32), ()>>,
    /// Cached function: on_beam_complete(ptr, len, success)
    fn_on_beam_complete: Option<TypedFunc<(i32, i32, i32), ()>>,
    /// Cached function: transform_command(ptr, len) -> ptr, len
    fn_transform_command: Option<TypedFunc<(i32, i32), (i32, i32)>>,
    /// Cached function: alloc(size) -> ptr
    fn_alloc: Option<TypedFunc<i32, i32>>,
    /// Cached function: dealloc(ptr, size)
    fn_dealloc: Option<TypedFunc<(i32, i32), ()>>,
}

impl PluginInstance {
    /// Creates a new plugin instance.
    fn new(engine: &Engine, plugin: &Plugin) -> Result<Self> {
        Self::with_state(engine, plugin, PluginState::new())
    }

    /// Creates a new plugin instance with custom state.
    fn with_state(engine: &Engine, plugin: &Plugin, state: PluginState) -> Result<Self> {
        let mut store = Store::new(engine, StoreData::new(state));
        let mut linker = Linker::new(engine);

        // Add host functions
        Self::add_host_functions(&mut linker)?;

        // Instantiate the module
        let instance = linker.instantiate(&mut store, &plugin.module)?;

        // Get memory export if available
        if let Some(memory) = instance.get_memory(&mut store, "memory") {
            store.data_mut().memory = Some(memory);
        }

        // Cache exported functions
        let fn_plugin_name = instance
            .get_typed_func::<(), (i32, i32)>(&mut store, "plugin_name")
            .ok();

        let fn_plugin_version = instance
            .get_typed_func::<(), (i32, i32)>(&mut store, "plugin_version")
            .ok();

        let fn_on_beam_start = instance
            .get_typed_func::<(i32, i32), ()>(&mut store, "on_beam_start")
            .ok();

        let fn_on_beam_complete = instance
            .get_typed_func::<(i32, i32, i32), ()>(&mut store, "on_beam_complete")
            .ok();

        let fn_transform_command = instance
            .get_typed_func::<(i32, i32), (i32, i32)>(&mut store, "transform_command")
            .ok();

        let fn_alloc = instance
            .get_typed_func::<i32, i32>(&mut store, "alloc")
            .ok();

        let fn_dealloc = instance
            .get_typed_func::<(i32, i32), ()>(&mut store, "dealloc")
            .ok();

        Ok(Self {
            store,
            fn_plugin_name,
            fn_plugin_version,
            fn_on_beam_start,
            fn_on_beam_complete,
            fn_transform_command,
            fn_alloc,
            fn_dealloc,
        })
    }

    /// Adds Aurora host functions to the linker.
    fn add_host_functions(linker: &mut Linker<StoreData>) -> Result<()> {
        // aurora_log(level: i32, ptr: i32, len: i32)
        linker.func_wrap(
            "aurora",
            "aurora_log",
            |mut caller: Caller<'_, StoreData>, level: i32, ptr: i32, len: i32| {
                if let Some(message) = read_string_from_memory(&mut caller, ptr, len) {
                    caller.data().state.log(level, &message);
                }
            },
        )?;

        // aurora_get_var(ptr: i32, len: i32) -> i64 (packed ptr, len)
        linker.func_wrap(
            "aurora",
            "aurora_get_var",
            |mut caller: Caller<'_, StoreData>, ptr: i32, len: i32| -> i64 {
                let name = read_string_from_memory(&mut caller, ptr, len).unwrap_or_default();
                let value = caller.data().state.get_var(&name).unwrap_or_default();

                // For simplicity, we return 0 if can't write. Real impl needs alloc.
                if let Some(result_ptr) = write_string_to_memory(&mut caller, &value) {
                    pack_ptr_len(result_ptr, value.len() as i32)
                } else {
                    0
                }
            },
        )?;

        // aurora_set_var(name_ptr: i32, name_len: i32, val_ptr: i32, val_len: i32)
        linker.func_wrap(
            "aurora",
            "aurora_set_var",
            |mut caller: Caller<'_, StoreData>,
             name_ptr: i32,
             name_len: i32,
             val_ptr: i32,
             val_len: i32| {
                let name = read_string_from_memory(&mut caller, name_ptr, name_len);
                let value = read_string_from_memory(&mut caller, val_ptr, val_len);

                if let (Some(name), Some(value)) = (name, value) {
                    caller.data().state.set_var(&name, &value);
                }
            },
        )?;

        // aurora_get_env(ptr: i32, len: i32) -> i64 (packed ptr, len)
        linker.func_wrap(
            "aurora",
            "aurora_get_env",
            |mut caller: Caller<'_, StoreData>, ptr: i32, len: i32| -> i64 {
                let name = read_string_from_memory(&mut caller, ptr, len).unwrap_or_default();
                let value = caller.data().state.get_env(&name).unwrap_or_default();

                if let Some(result_ptr) = write_string_to_memory(&mut caller, &value) {
                    pack_ptr_len(result_ptr, value.len() as i32)
                } else {
                    0
                }
            },
        )?;

        Ok(())
    }

    /// Gets the plugin name from the WASM module.
    pub fn plugin_name(&mut self) -> Result<Option<String>> {
        let Some(ref func) = self.fn_plugin_name else {
            return Ok(None);
        };

        let (ptr, len) = func.call(&mut self.store, ())?;
        Ok(read_string_from_memory_store(&mut self.store, ptr, len))
    }

    /// Gets the plugin version from the WASM module.
    pub fn plugin_version(&mut self) -> Result<Option<String>> {
        let Some(ref func) = self.fn_plugin_version else {
            return Ok(None);
        };

        let (ptr, len) = func.call(&mut self.store, ())?;
        Ok(read_string_from_memory_store(&mut self.store, ptr, len))
    }

    /// Called before a beam starts execution.
    pub fn on_beam_start(&mut self, beam_name: &str) -> Result<()> {
        // Clone the function reference to avoid borrow conflict
        let func = match self.fn_on_beam_start.clone() {
            Some(f) => f,
            None => return Ok(()),
        };

        let (ptr, len) = self.write_string(beam_name)?;
        func.call(&mut self.store, (ptr, len))?;
        self.free_string(ptr, len)?;
        Ok(())
    }

    /// Called after a beam completes execution.
    pub fn on_beam_complete(&mut self, beam_name: &str, success: bool) -> Result<()> {
        // Clone the function reference to avoid borrow conflict
        let func = match self.fn_on_beam_complete.clone() {
            Some(f) => f,
            None => return Ok(()),
        };

        let (ptr, len) = self.write_string(beam_name)?;
        func.call(&mut self.store, (ptr, len, if success { 1 } else { 0 }))?;
        self.free_string(ptr, len)?;
        Ok(())
    }

    /// Transforms a command before execution.
    pub fn transform_command(&mut self, command: &str) -> Result<Option<String>> {
        // Clone the function reference to avoid borrow conflict
        let func = match self.fn_transform_command.clone() {
            Some(f) => f,
            None => return Ok(None),
        };

        let (in_ptr, in_len) = self.write_string(command)?;
        let (out_ptr, out_len) = func.call(&mut self.store, (in_ptr, in_len))?;
        self.free_string(in_ptr, in_len)?;

        let result = read_string_from_memory_store(&mut self.store, out_ptr, out_len);
        if out_len > 0 {
            self.free_string(out_ptr, out_len)?;
        }

        Ok(result)
    }

    /// Gets the plugin state.
    pub fn state(&self) -> &PluginState {
        &self.store.data().state
    }

    /// Gets a mutable reference to the plugin state.
    pub fn state_mut(&mut self) -> &mut PluginState {
        &mut self.store.data_mut().state
    }

    /// Writes a string to WASM memory using the alloc function.
    fn write_string(&mut self, s: &str) -> Result<(i32, i32)> {
        let bytes = s.as_bytes();
        let len = bytes.len() as i32;

        let ptr = if let Some(ref alloc) = self.fn_alloc {
            alloc.call(&mut self.store, len)?
        } else {
            return Err(PluginError::FunctionNotFound("alloc".to_string()));
        };

        // Write to memory
        if let Some(memory) = self.store.data().memory {
            let mem_data = memory.data_mut(&mut self.store);
            let start = ptr as usize;
            let end = start + bytes.len();
            if end <= mem_data.len() {
                mem_data[start..end].copy_from_slice(bytes);
            }
        }

        Ok((ptr, len))
    }

    /// Frees a string from WASM memory using the dealloc function.
    fn free_string(&mut self, ptr: i32, len: i32) -> Result<()> {
        if let Some(ref dealloc) = self.fn_dealloc {
            dealloc.call(&mut self.store, (ptr, len))?;
        }
        Ok(())
    }
}

/// Reads a string from WASM memory via Caller.
fn read_string_from_memory(
    caller: &mut Caller<'_, StoreData>,
    ptr: i32,
    len: i32,
) -> Option<String> {
    let memory = caller.data().memory?;
    let data = memory.data(caller);
    let start = ptr as usize;
    let end = start + len as usize;

    if end > data.len() {
        return None;
    }

    String::from_utf8(data[start..end].to_vec()).ok()
}

/// Reads a string from WASM memory via Store.
fn read_string_from_memory_store(
    store: &mut Store<StoreData>,
    ptr: i32,
    len: i32,
) -> Option<String> {
    let memory = store.data().memory?;
    let data = memory.data(store);
    let start = ptr as usize;
    let end = start + len as usize;

    if end > data.len() {
        return None;
    }

    String::from_utf8(data[start..end].to_vec()).ok()
}

/// Writes a string to WASM memory and returns the pointer.
/// Note: This is a simplified version that writes to a fixed location.
/// Real implementation should use the plugin's alloc function.
fn write_string_to_memory(caller: &mut Caller<'_, StoreData>, s: &str) -> Option<i32> {
    let memory = caller.data().memory?;
    let bytes = s.as_bytes();

    // Find a suitable location (this is simplified - real impl needs proper allocation)
    // We'll use a high memory address that's unlikely to conflict
    let ptr = 0x10000i32;

    let data = memory.data_mut(caller);
    let start = ptr as usize;
    let end = start + bytes.len();

    if end > data.len() {
        return None;
    }

    data[start..end].copy_from_slice(bytes);
    Some(ptr)
}

/// Packs a pointer and length into a single i64.
fn pack_ptr_len(ptr: i32, len: i32) -> i64 {
    ((ptr as i64) << 32) | (len as i64 & 0xFFFFFFFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_runtime() {
        let runtime = PluginRuntime::new().unwrap();
        assert!(runtime.plugins().next().is_none());
    }

    #[test]
    fn test_pack_ptr_len() {
        let packed = pack_ptr_len(100, 50);
        let ptr = (packed >> 32) as i32;
        let len = (packed & 0xFFFFFFFF) as i32;
        assert_eq!(ptr, 100);
        assert_eq!(len, 50);
    }

    #[test]
    fn test_load_nonexistent_plugin() {
        let mut runtime = PluginRuntime::new().unwrap();
        let result = runtime.load_plugin(Path::new("/nonexistent/path"));
        assert!(result.is_err());
    }

    // A minimal valid WASM module (empty)
    const MINIMAL_WASM: &[u8] = &[
        0x00, 0x61, 0x73, 0x6d, // magic
        0x01, 0x00, 0x00, 0x00, // version
    ];

    #[test]
    fn test_load_plugin_from_bytes() {
        let mut runtime = PluginRuntime::new().unwrap();
        let plugin = runtime
            .load_plugin_from_bytes("test", "1.0.0", MINIMAL_WASM)
            .unwrap();

        assert_eq!(plugin.name(), "test");
        assert_eq!(plugin.version(), "1.0.0");

        // Should be able to retrieve it
        let retrieved = runtime.get_plugin("test").unwrap();
        assert_eq!(retrieved.name(), "test");
    }

    #[test]
    fn test_unload_plugin() {
        let mut runtime = PluginRuntime::new().unwrap();
        runtime
            .load_plugin_from_bytes("test", "1.0.0", MINIMAL_WASM)
            .unwrap();

        assert!(runtime.get_plugin("test").is_some());
        assert!(runtime.unload_plugin("test"));
        assert!(runtime.get_plugin("test").is_none());
        assert!(!runtime.unload_plugin("test")); // Already removed
    }

    #[test]
    fn test_create_instance() {
        let mut runtime = PluginRuntime::new().unwrap();
        let plugin = runtime
            .load_plugin_from_bytes("test", "1.0.0", MINIMAL_WASM)
            .unwrap();

        let instance = runtime.create_instance(&plugin).unwrap();

        // Minimal WASM has no exports, so these should return None/Ok
        assert!(instance.fn_plugin_name.is_none());
        assert!(instance.fn_plugin_version.is_none());
    }
}
