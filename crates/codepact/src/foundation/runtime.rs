//! Applicable standard library profiles and their reviewed model catalog.

use serde::{Deserialize, Serialize};

pub const CATALOG_REVISION: &str = "cpython-3.13.16-linux-x86_64-v1";

/// The runtime of the analyzed program. The Python grammar target is a
/// separate input and the checker never derives these values from its host.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct RuntimeProfile {
    pub implementation: String,
    pub python_version: String,
    pub os: String,
    pub architecture: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_loop: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_method: Option<String>,
    #[serde(default)]
    pub assume_standard_process_wait: bool,
}

impl RuntimeProfile {
    pub(crate) fn applicability(&self, target: &str) -> Result<(), &'static str> {
        if self.python_version.rsplit_once('.').map(|(minor, _)| minor) != Some(target) {
            return Err("the analyzed runtime and Python source target disagree");
        }
        if self.implementation != "cpython"
            || self.python_version != "3.13.16"
            || self.os != "linux"
            || self.architecture != "x86_64"
        {
            return Err("the model requires CPython 3.13.16 on Linux x86_64");
        }
        Ok(())
    }

    pub(crate) fn asyncio(&self, target: &str, default_factory: bool) -> Result<(), &'static str> {
        self.applicability(target)?;
        if self.event_loop.as_deref() != Some("default") || !default_factory {
            return Err(
                "the asyncio model requires the default event loop and non-eager task factory",
            );
        }
        Ok(())
    }

    pub(crate) fn spawn(&self, target: &str, method: &str) -> Result<(), &'static str> {
        self.applicability(target)?;
        if method != "spawn"
            || self
                .start_method
                .as_deref()
                .is_some_and(|value| value != method)
        {
            return Err("the multiprocessing model requires an explicit compatible spawn context");
        }
        Ok(())
    }
}

pub(crate) fn stdlib_catalog(
    profile: &RuntimeProfile,
    target: &str,
    ownership: crate::Ownership,
) -> Vec<crate::CheckModelMetadata> {
    if profile.applicability(target).is_err() {
        return Vec::new();
    }
    let mut catalog = vec![(
        "TPM-FILE-001",
        "builtins.open",
        "Modules/_io/textio.c",
        "Ordinary text files, local aliasing, close and context exit.",
    )];
    if profile
        .asyncio(target, ownership.assumes_default_task_factory())
        .is_ok()
    {
        catalog.push((
            "TPM-ASYNCIO-001",
            "asyncio.create_task",
            "Lib/asyncio/tasks.py",
            "Local scheduling, direct wait, cancellation, shield and timeout.",
        ));
    }
    if ownership.includes_advanced_contracts() && profile.spawn(target, "spawn").is_ok() {
        catalog.extend([
            (
                "TPM-PROCESS-001",
                "multiprocessing.Process",
                "Lib/multiprocessing/process.py",
                "Explicit-spawn local process transitions, qualified join and close.",
            ),
            (
                "TPM-POOL-001",
                "multiprocessing.Pool",
                "Lib/multiprocessing/pool.py",
                "Explicit-spawn submission and local Pool shutdown.",
            ),
        ]);
    }
    catalog.into_iter().map(|(id, symbol, source, behavior)| crate::CheckModelMetadata {
        id: id.into(),
        qualified_symbol: symbol.into(),
        source_revisions: vec!["v3.13.16".into()],
        source_urls: vec![format!("https://github.com/python/cpython/blob/v3.13.16/{source}")],
        selection_rule: "Declared CPython 3.13.16/Linux x86_64 runtime, exact binding and applicable model assumptions.".into(),
        modeled_behavior: behavior.into(),
        qualification: "Phase 0.8 experimental model. Release qualification is tracked in the phase plan.".into(),
    }).collect()
}
