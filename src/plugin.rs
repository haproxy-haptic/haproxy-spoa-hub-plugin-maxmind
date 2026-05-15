use std::sync::OnceLock;
use std::time::Duration;

use haproxy_spoa_hub_plugin_api::{
    MetricRecorder, ProcessingResult, RVec, SpoeMessage, SpoeValue, TxnVariable, define_plugin,
};

use crate::config::PluginConfig;
use crate::ip::spoe_value_to_ip;
use crate::lookup::{LoadedDatabases, LookupRouter, LookupStatus, execute_lookup, load};
use crate::warn_log::WarnOnce;

const WARN_DEDUP_WINDOW: Duration = Duration::from_secs(300);

/// Plugin-scoped metric recorder; silent no-op on pre-v0.6 hubs.
static METRICS: MetricRecorder = MetricRecorder::new();

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
        let arg_value = message.get(&lookup.input).ok_or_else(|| {
            format!(
                "lookup '{}' missing required argument '{}'",
                lookup.name, lookup.input
            )
        })?;

        let started = std::time::Instant::now();
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
        let elapsed = started.elapsed();

        // Hub adds `plugin_maxmind_` prefix. `lookup` is the lookup
        // name (per-config), `status` is one of LookupStatus's
        // bounded set (hit/miss/error). The histogram is unlabelled
        // by lookup because most ops care about overall lookup
        // latency, not per-lookup-name p99 tails.
        METRICS.counter(
            "lookups_total",
            &[("lookup", &*lookup.name), ("status", status.as_str())],
            1,
        );
        METRICS.histogram("lookup_duration_seconds", &[], elapsed.as_secs_f64());

        variables.push(TxnVariable::transaction(
            &*lookup.output_var,
            SpoeValue::String(value.into()),
        ));
        variables.push(TxnVariable::transaction(
            &*lookup.status_var,
            SpoeValue::String(status.as_str().into()),
        ));
    }

    Ok(variables)
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

    metrics_static = METRICS;
});
