//! Applicable standard library profiles and their reviewed model catalog.

use serde::{Deserialize, Serialize};

pub const CATALOG_REVISION: &str = "cpython-3.13.16-linux-x86_64-v3";

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
    /// Exact source path of the script invoked by the interpreter for spawn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spawn_main_script: Option<String>,
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
        catalog.extend([
            (
                "TPM-ASYNCIO-001",
                "asyncio.create_task",
                "Lib/asyncio/tasks.py",
                "Local scheduling, direct wait, cancellation, shield and timeout.",
            ),
            (
                "TPM-QUEUE-001",
                "asyncio.Queue",
                "Lib/asyncio/queues.py",
                "FIFO admission, receipt, accounting, join and shutdown.",
            ),
            (
                "TPM-SYNC-001",
                "asyncio.Lock",
                "Lib/asyncio/locks.py",
                "Local guards, semaphore permits, condition waits and event signals.",
            ),
            (
                "TPM-BRIDGE-001",
                "asyncio.to_thread",
                "Lib/asyncio/threads.py",
                "Dispatch, wrapper cancellation, and result delivery across executor and event-loop bridges.",
            ),
            (
                "TPM-ASYNCGEN-001",
                "contextlib.aclosing",
                "Lib/contextlib.py",
                "Source-resolved async generator acquisition, suspension, exhaustion and cleanup.",
            ),
            (
                "TPM-ASYNCIO-IO-001",
                "asyncio.open_connection",
                "Lib/asyncio/streams.py",
                "Local stream, server and subprocess shutdown and closure evidence.",
            ),
        ]);
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
            (
                "TPM-IPC-001",
                "multiprocessing.Queue",
                "Lib/multiprocessing/queues.py",
                "Queue and JoinableQueue admission, receipt, accounting and feeder cleanup.",
            ),
            (
                "TPM-IPC-002",
                "multiprocessing.connection.Connection",
                "Lib/multiprocessing/connection.py",
                "SimpleQueue and Pipe serialization, local endpoints, receipt and EOF.",
            ),
            (
                "TPM-SYNC-002",
                "multiprocessing.Lock",
                "Lib/multiprocessing/synchronize.py",
                "Process locks, semaphore permits and guards for shared values and arrays.",
            ),
            (
                "TPM-SHM-001",
                "multiprocessing.shared_memory.SharedMemory",
                "Lib/multiprocessing/shared_memory.py",
                "Local mappings and exported views, name removal and the designated unlink duty.",
            ),
        ]);
        if profile.spawn_main_script.is_some() {
            catalog.push((
                "TPM-SPAWN-001",
                "multiprocessing.Process.start",
                "Lib/multiprocessing/spawn.py",
                "Direct-script spawn entry, worker resolution and argument checks.",
            ));
        }
    }
    catalog.into_iter().map(|(id, symbol, source, behavior)| {
        let supporting_sources: &[&str] = match id {
            "TPM-QUEUE-001" => &["Lib/asyncio/locks.py"],
            "TPM-IPC-001" => &["Lib/multiprocessing/connection.py", "Lib/multiprocessing/reduction.py"],
            "TPM-IPC-002" => &["Lib/multiprocessing/queues.py", "Lib/multiprocessing/reduction.py"],
            "TPM-SYNC-002" => &["Lib/multiprocessing/sharedctypes.py", "Modules/_multiprocessing/semaphore.c"],
            "TPM-SPAWN-001" => &["Lib/multiprocessing/popen_spawn_posix.py", "Lib/multiprocessing/process.py"],
            "TPM-SHM-001" => &["Lib/multiprocessing/resource_tracker.py", "Modules/_multiprocessing/posixshmem.c"],
            "TPM-BRIDGE-001" => &["Lib/asyncio/base_events.py", "Lib/asyncio/tasks.py", "Lib/asyncio/futures.py", "Lib/concurrent/futures/thread.py", "Lib/concurrent/futures/process.py"],
            "TPM-ASYNCGEN-001" => &["Objects/genobject.c", "Lib/asyncio/base_events.py"],
            "TPM-ASYNCIO-IO-001" => &["Lib/asyncio/base_events.py", "Lib/asyncio/subprocess.py", "Lib/asyncio/base_subprocess.py", "Lib/asyncio/unix_events.py"],
            _ => &[],
        };
        crate::CheckModelMetadata {
            id: id.into(),
            qualified_symbol: symbol.into(),
            source_revisions: vec!["v3.13.16".into()],
            source_urls: std::iter::once(source)
                .chain(supporting_sources.iter().copied())
                .map(|path| format!("https://github.com/python/cpython/blob/v3.13.16/{path}"))
                .collect(),
            selection_rule: if id == "TPM-SPAWN-001" {
                "Declared CPython 3.13.16/Linux x86_64 runtime, explicit spawn and matching direct script."
            } else {
                "Declared CPython 3.13.16/Linux x86_64 runtime, exact binding and applicable model assumptions."
            }.into(),
            modeled_behavior: behavior.into(),
            qualification: "Experimental model. Release qualification is tracked in the phase plan.".into(),
        }
    }).collect()
}
