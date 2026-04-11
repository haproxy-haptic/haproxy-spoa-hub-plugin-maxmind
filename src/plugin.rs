use std::sync::OnceLock;

use abi_stable::std_types::RVec;
use haproxy_spoa_hub_plugin_api::{
    ProcessingResult, SpoeValue, TxnVariable, VarScope, define_plugin,
};

use crate::config::PluginConfig;
use crate::ip::extract_ip;
use crate::lookup::{LoadedDatabase, load_databases, lookup_ip};

struct PluginState {
    config: PluginConfig,
    databases: Vec<LoadedDatabase>,
}

static STATE: OnceLock<PluginState> = OnceLock::new();

#[derive(Debug)]
struct MaxmindPlugin;

define_plugin!(MaxmindPlugin, {
    fn new() -> Self {
        MaxmindPlugin
    }

    fn init(
        &mut self,
        context: &PluginContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let config = PluginConfig::from_context(context)
            .map_err(|e| format!("maxmind plugin configuration error: {e}"))?;

        let databases = load_databases(&config.databases)?;

        log::info!("maxmind plugin loaded {} database(s)", databases.len());

        STATE.set(PluginState { config, databases }).ok();
        Ok(())
    }

    fn name(&self) -> &str {
        "maxmind"
    }

    fn version(&self) -> &str {
        env!("CARGO_PKG_VERSION")
    }

    fn process(
        &self,
        message: &SpoeMessage,
    ) -> Result<ProcessingResult, Box<dyn std::error::Error + Send + Sync>> {
        let state = STATE.get().ok_or("maxmind plugin not initialized")?;

        let ip = extract_ip(message)?;

        let fields = lookup_ip(
            ip,
            &state.databases,
            &state.config.locale,
            state.config.fields.as_deref(),
        );

        let variables = fields
            .into_iter()
            .map(|(name, value)| TxnVariable {
                scope: VarScope::Transaction,
                name: abi_stable::std_types::RString::from(name.as_str()),
                value: SpoeValue::String(abi_stable::std_types::RString::from(value.as_str())),
            })
            .collect::<RVec<_>>();

        Ok(ProcessingResult { variables })
    }

    fn config_schema(&self) -> Option<&str> {
        Some(PluginConfig::config_schema())
    }
});
