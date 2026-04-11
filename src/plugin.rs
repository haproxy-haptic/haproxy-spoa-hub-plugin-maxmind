use std::sync::OnceLock;
use std::time::Duration;

use abi_stable::std_types::{RString, RVec, Tuple2};
use haproxy_spoa_hub_plugin_api::{
    ProcessingResult, SpoeMessage, SpoeValue, TxnVariable, VarScope, define_plugin,
};

use crate::config::PluginConfig;
use crate::ip::spoe_value_to_ip;
use crate::lookup::{LoadedDatabases, LookupRouter, LookupStatus, execute_lookup, load};
use crate::warn_log::WarnOnce;

const WARN_DEDUP_WINDOW: Duration = Duration::from_secs(300);

pub struct PluginState {
    pub databases: LoadedDatabases,
    pub router: LookupRouter,
    pub warn_once: WarnOnce,
}

static STATE: OnceLock<PluginState> = OnceLock::new();

#[derive(Debug)]
struct MaxmindPlugin;

/// Core message-processing logic, factored out so integration tests can call it
/// without going through the `OnceLock`-backed plugin singleton.
pub fn process_message(
    databases: &LoadedDatabases,
    router: &LookupRouter,
    warn_once: &WarnOnce,
    message: &SpoeMessage,
) -> Result<RVec<TxnVariable>, String> {
    let indices = router
        .lookups_for(message.name.as_str())
        .ok_or_else(|| format!("no lookup bound to message '{}'", message.name.as_str()))?;

    let mut variables: RVec<TxnVariable> = RVec::new();

    for &idx in indices {
        let lookup = router.get(idx);

        // Find the configured input argument; missing argument is a hard
        // hub-level error per FR-008 / contracts doc.
        let arg_value = find_arg(message, &lookup.input).ok_or_else(|| {
            format!(
                "lookup '{}' missing required argument '{}'",
                lookup.name, lookup.input
            )
        })?;

        let (value, status) = match spoe_value_to_ip(arg_value) {
            Ok(ip) => execute_lookup(lookup, databases, ip, warn_once),
            Err(e) => {
                if warn_once.should_warn(&lookup.name, &lookup.path_display) {
                    log::warn!(
                        "lookup '{}': invalid input value for argument '{}': {e}",
                        lookup.name,
                        lookup.input
                    );
                }
                (String::new(), LookupStatus::Error)
            }
        };

        variables.push(string_var(&lookup.output_var, &value));
        variables.push(string_var(&lookup.status_var, status.as_str()));
    }

    Ok(variables)
}

fn find_arg<'a>(message: &'a SpoeMessage, name: &str) -> Option<&'a SpoeValue> {
    for Tuple2(k, v) in message.args.iter() {
        if k.as_str() == name {
            return Some(v);
        }
    }
    None
}

fn string_var(name: &str, value: &str) -> TxnVariable {
    TxnVariable {
        scope: VarScope::Transaction,
        name: RString::from(name),
        value: SpoeValue::String(RString::from(value)),
    }
}

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

        let n_dbs = config.databases.len();
        let n_lookups = config.lookups.len();
        let (databases, router) = load(config).map_err(|e| format!("maxmind plugin: {e}"))?;

        log::info!(
            "maxmind plugin loaded {} database(s) and {} lookup(s)",
            n_dbs,
            n_lookups
        );

        STATE
            .set(PluginState {
                databases,
                router,
                warn_once: WarnOnce::new(WARN_DEDUP_WINDOW),
            })
            .ok();
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
        let variables =
            process_message(&state.databases, &state.router, &state.warn_once, message)?;
        Ok(ProcessingResult { variables })
    }

    fn config_schema(&self) -> Option<&str> {
        Some(PluginConfig::config_schema())
    }
});
