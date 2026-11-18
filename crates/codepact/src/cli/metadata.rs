use super::discovery;
use codepact::adoption::EffectiveConfig;

pub(super) fn settings_json(c: &EffectiveConfig) -> Result<String, String> {
    let root = discovery::resolve_project_root(&c.root)?;
    let mut settings = serde_json::json!({
        "execution_status": "settings-resolved",
        "settings_schema_version": 2,
        "root": root,
        "config_file": c.source,
        "python_version": c.python_version,
        "include": c.include,
        "exclude": c.exclude,
        "enabled_rules": c.enabled_rules,
        "mode": c.mode,
        "experimental_ownership": c.ownership.is_enabled(),
        "experimental_advanced_contracts": c.ownership.includes_advanced_contracts(),
        "experimental_finite_matches": c.finite_matches,
        "dependency_roots": c.dependency_roots,
        "required_providers": c.required_providers,
        "assume_default_task_factory": c.ownership.assumes_default_task_factory(),
        "fail_on": c.fail_on,
        "baseline": c.baseline,
        "severity": c.severity,
        "settings_origins": c.settings_origins
    });
    if let Some(profile) = &c.runtime_profile {
        settings["runtime_profile"] = serde_json::to_value(profile).unwrap();
    }
    if c.experimental_foundations {
        settings["experimental_foundations"] = serde_json::Value::Bool(true);
    }
    if !c.project_contracts.is_empty() {
        settings["project_contracts"] = serde_json::to_value(&c.project_contracts).unwrap();
    }
    if let Some(lock) = &c.uv_lock {
        settings["uv_lock"] = serde_json::to_value(lock).unwrap();
    }
    if !c.declaration_policy.is_empty() {
        settings["declaration_policy"] = serde_json::to_value(&c.declaration_policy).unwrap();
    }
    if !c.async_guards.is_empty() {
        settings["async_guards"] = serde_json::to_value(&c.async_guards).unwrap();
    }
    if c.experimental_foundations
        || c.runtime_profile.is_some()
        || !c.project_contracts.is_empty()
        || c.uv_lock.is_some()
        || !c.declaration_policy.is_empty()
        || !c.async_guards.is_empty()
    {
        settings["settings_schema_version"] = serde_json::Value::from(3);
    }
    Ok(serde_json::to_string_pretty(&settings).unwrap())
}

pub(super) fn capabilities_json() -> String {
    let mut capabilities = serde_json::json!({
        "capability_schema_version": 1,
        "engine_version": env!("CARGO_PKG_VERSION"),
        "checker_schema_version": 2,
        "baseline_schema_version": 1,
        "inspection_schema_version": 0,
        "explanation_schema_version": 1,
        "python_targets": ["3.11", "3.12", "3.13"],
        "rules": [
            {"code":"TP1001","semantic_version":1,"tier":"stable","enabled_by_default":true,"explanation_id":"TP1001.R001"},
            {"code":"TP1002","semantic_version":1,"tier":"stable","enabled_by_default":true,"explanation_id":"TP1002.R001"},
            {"code":"TP1003","semantic_version":1,"tier":"stable","enabled_by_default":true,"explanation_id":"TP1003.R001"},
            {"code":"TP2001","semantic_version":1,"tier":"stable","enabled_by_default":true,"explanation_id":"TP2001.R001"},
            {"code":"TP3001","semantic_version":1,"tier":"stable","enabled_by_default":true,"explanation_id":"TP3001.R001"},
            {"code":"TP3003","semantic_version":1,"tier":"experimental","enabled_by_default":false,"explanation_id":"TP3003.R001"},
            {"code":"TP6001","semantic_version":1,"tier":"review-hint","enabled_by_default":true,"explanation_id":"TP6001.R001"},
            {"code":"TP6002","semantic_version":1,"tier":"review-hint","enabled_by_default":true,"explanation_id":"TP6002.R001"},
            {"code":"TP6003","semantic_version":1,"tier":"review-hint","enabled_by_default":true,"explanation_id":"TP6003.R001"},
            {"code":"TP4001","semantic_version":1,"tier":"mandatory","enabled_by_default":true,"explanation_id":"TP4001.R001"},
            {"code":"TP4002","semantic_version":1,"tier":"mandatory","enabled_by_default":true,"explanation_id":"TP4002.R001"},
            {"code":"TP4101","semantic_version":1,"tier":"experimental","enabled_by_default":false,"explanation_id":"TP4101.R001"},
            {"code":"TP4102","semantic_version":1,"tier":"experimental","enabled_by_default":false,"explanation_id":"TP4102.R001"},
            {"code":"TP4103","semantic_version":1,"tier":"experimental","enabled_by_default":false,"explanation_id":"TP4103.R001"},
            {"code":"TP7001","semantic_version":1,"tier":"experimental","enabled_by_default":false,"explanation_id":"TP7001.R001"},
            {"code":"TP7002","semantic_version":1,"tier":"experimental","enabled_by_default":false,"explanation_id":"TP7002.R001"},
            {"code":"TP7003","semantic_version":1,"tier":"experimental","enabled_by_default":false,"explanation_id":"TP7003.R001"},
            {"code":"TP7004","semantic_version":1,"tier":"experimental","enabled_by_default":false,"explanation_id":"TP7004.R001"},
            {"code":"TP7005","semantic_version":1,"tier":"experimental","enabled_by_default":false,"explanation_id":"TP7005.R001"},
            {"code":"TP9001","semantic_version":1,"tier":"mandatory","enabled_by_default":true,"explanation_id":"TP9001.R001"},
            {"code":"TP9002","semantic_version":1,"tier":"mandatory","enabled_by_default":true,"explanation_id":"TP9002.R001"}
        ],
        "formats": ["text", "json", "concise"],
        "settings": ["root", "python-version", "include", "exclude", "enabled-rules", "mode", "fail-on", "baseline", "severity", "experimental-ownership", "experimental-advanced-contracts", "experimental-finite-matches", "dependency-roots", "required-providers", "assume-default-task-factory", "experimental-foundations", "runtime-profile", "project-contracts", "uv-lock", "declaration-policy", "async-guards"],
        "limits": {
            "config_bytes": 65536,
            "baseline_bytes": 16777216,
            "baseline_entries": 100000,
            "report_bytes": 33554432,
            "related_sites_per_finding": 32,
            "source_snippet_bytes_per_finding": 2048,
            "explanation_bytes_per_finding": 8192,
            "max_discovery_entries": discovery::MAX_DISCOVERY_ENTRIES,
            "max_directory_entries": discovery::MAX_DIRECTORY_ENTRIES,
            "max_discovery_depth": discovery::MAX_DISCOVERY_DEPTH,
            "max_input_files": codepact::DEFAULT_MAX_INPUT_FILES,
            "max_source_bytes": codepact::DEFAULT_MAX_SOURCE_BYTES
        },
        "services": {"pagination": false,"mcp": false,"language_server": false,"persistent_cache": false}
    });
    capabilities["rules"].as_array_mut().unwrap().extend(codepact::adoption::OWNERSHIP_RULES.iter().map(|code| {
            serde_json::json!({"code":code,"semantic_version":1,"tier":"experimental","enabled_by_default":false,"explanation_id":format!("{code}.R001")})
        }));
    capabilities["rules"].as_array_mut().unwrap().extend(codepact::adoption::ADVANCED_RULES.iter().map(|code| {
            serde_json::json!({"code":code,"semantic_version":1,"tier":"experimental","enabled_by_default":false,"explanation_id":format!("{code}.R001")})
        }));
    capabilities["rules"].as_array_mut().unwrap().extend(codepact::adoption::FINITE_MATCH_RULES.iter().map(|code| {
            serde_json::json!({"code":code,"semantic_version":1,"tier":"experimental","enabled_by_default":false,"explanation_id":format!("{code}.R001")})
        }));
    capabilities["rules"].as_array_mut().unwrap().extend(codepact::adoption::OUTCOME_RULES.iter().map(|code| {
        serde_json::json!({"code":code,"semantic_version":1,"tier":"experimental","enabled_by_default":true,"explanation_id":format!("{code}.R001")})
    }));
    serde_json::to_string_pretty(&capabilities).unwrap()
}

pub(super) fn explanation_json(rule: &str) -> Option<String> {
    let explanation = match rule {
            "TP1001" => ("Use after close", "A modeled resource use occurs after its close transition.", "Keep the use before close or guard it with a lifecycle check.", "A matching resource close transition is resolved in the supported local flow model.", "The model does not prove arbitrary runtime dispatch or external library behavior.", "a.close(); a.read()", "a.read(); a.close()", "resource acquisition and close summary is unresolved", "Move the use before close or establish a supported lifecycle guard."),
            "TP1002" => ("Open cleanup duty", "An owned resource remains open on a modeled function exit.", "Close it on every reported exit or return it through a resolved Owned[T] boundary.", "A supported owned resource is created or transferred into the function.", "Unknown exceptions, dynamic escapes and unsupported concurrency remain coverage boundaries.", "def f():\n    r = Resource()\n    r.close()", "def f():\n    r = Resource()", "the ownership or exit path is not modeled", "Close on each feasible exit or return ownership through the declared contract."),
            "TP1003" => ("Invalid owned return", "An Owned[T] result violates the modeled handoff contract.", "Return a valid open owned resource after cleanup paths have completed.", "The return annotation resolves to the supported Owned[T] contract.", "This does not validate the runtime type or behavior of the returned object.", "return open_resource", "close_resource(); return resource", "the returned resource identity or handoff is unresolved", "Return the live owned resource after local cleanup."),
            "TP2001" => ("Close through borrow", "A borrowed resource is closed through a non-owning view.", "Leave cleanup with the owning caller.", "The borrow and close operation resolve to the same modeled resource identity.", "Borrow escape and dynamic dispatch beyond the supported flow subset are not proven.", "borrow.read()", "borrow.close()", "the receiver's ownership identity is uncertain", "Remove the close through the borrow and return cleanup responsibility to its owner."),
            "TP2002" => ("Use after move", "An operation uses a handle after it transferred ownership.", "Use the receiving handle or finish using the original before the move.", "A recognized codepact.move committed in the checked flow.", "Python still holds the original reference at runtime.", "dst = move(src)\ndst.close()", "dst = move(src)\nsrc.close()", "the move occurred on only some paths", "Use dst or move src after its last use."),
            "TP2003" => ("Invalid ownership transfer", "The transfer lacks an eligible owner or conflicts with a live alias or borrow.", "Keep one open owner and end conflicting uses before transferring it.", "The handle and tracked resource resolve at the transfer site.", "Unresolved aliases and captures can prevent proof of transfer.", "dst = move(src)\ndst.close()", "alias = src\ndst = move(src)", "the resource passed through an unknown callback", "End the alias or borrow before moving the owner."),
            "TP2004" => ("Borrow escape or consumption", "A checked borrow is consumed or leaves its lexical region.", "Keep the view local and leave cleanup with the owner.", "The borrowed view and its resource identity are tracked.", "Dynamic capture and dispatch can leave incomplete coverage.", "borrow.read()", "return borrow", "an unmodeled callback receives borrow", "Return derived data or use the view inside the region."),
            "TP2005" => ("Missing explicit move", "A resolved Owned parameter received a tracked resource without an explicit transfer.", "Use codepact.move for an intentional transfer.", "The parameter has a resolved Owned contract.", "Unresolved signatures cannot establish an owning receiver.", "consume(move(owner))", "consume(owner)", "the target may be a different callable", "Move the owner at the call site or use a verified Borrowed contract."),
            "TP2006" => ("Close with outstanding borrow", "Cleanup occurs while a tracked coroutine or task borrow is outstanding.", "Close after the modeled completion boundary.", "The capture and close resolve to the same modeled resource.", "The task model assumes CPython's default task factory and the documented TaskGroup subset.", "async with TaskGroup() as group:\n    group.create_task(read(owner))\nowner.close()", "async with TaskGroup() as group:\n    group.create_task(read(owner))\n    owner.close()", "a detached task or custom task factory", "Move cleanup after the group exits, including on exceptional paths."),
            "TP2101" => ("Conflicting exclusive access", "A tracked resource is accessed through a competing alias while an exclusive borrow is active.", "End the conflicting access before entering the exclusive region, or use the exclusive view for all access.", "The exclusive parameter and competing access resolve to the same modeled object.", "Dynamic aliases and unsupported callback effects can leave incomplete coverage.", "mutate(resource)", "mutate(resource, alias) where alias = resource", "the alias or call target is unresolved", "Pass only the exclusive view during its active region and avoid competing aliases."),
            "TP2102" => ("Invalid protocol state or inactive group", "A modeled operation may violate its finite state precondition, or a TaskGroup operation may occur outside the active group scope.", "Establish the required state or keep task submission between the group's first entry and completed exit.", "The object or TaskGroup identity resolves to a supported finite model.", "Unknown transitions, unresolved control flow, and external effects remain visible boundaries.", "if state == 'ready': send()", "send() when the object may be closed; group.create_task(...) after group exit", "an unknown call changes protocol state or the group lifetime is unresolved", "Establish the required state or use the tracked group only while active."),
            "TP3001" => ("Discarded must-use result", "A declared must-use result is discarded as an expression statement.", "Handle the value, assign it for immediate handling, or pass it to codepact.discard when abandonment is intentional.", "The synchronous callable resolves to a supported must-use declaration.", "Assignment is immediate handling only; later use or eventual consumption is not proven. Async and dynamic consumption are unqualified.", "value = fetch()", "fetch()", "the callee is dynamically selected or the result is nested in an unsupported expression", "Handle the result, or use codepact.discard(call()) when intentional discard is explicit."),
            "TP3003" => ("Unobserved work", "A locally submitted task, future, or pool job leaves scope without delivery of its value or worker exception.", "Observe the value or exception through a supported await or retrieval.", "The work handle and its operations resolve to a reviewed runtime profile.", "Readiness and shutdown alone provide completion evidence. Outcome acknowledgment is a separate duty.", "submitted = executor.submit(worker)\nsubmitted.result()", "executor.submit(worker)", "the handle is stored in an opaque container", "Retain the handle and observe each feasible delivery, including worker failure."),
            "TP5001" => ("Non-exhaustive outcome match", "A supported finite outcome match leaves a known variant uncovered.", "Add an unguarded arm for each remaining Ok/Err or Some/Nothing variant.", "The scrutinee resolves to a closed CodePact outcome variant set.", "Unsupported payload tests, open inputs and unresolved identities remain coverage boundaries.", "match result:\\n    case Ok(value): ...\\n    case Err(error): ...", "match result:\\n    case Ok(value): ...", "the scrutinee type or a reachable pattern is unresolved", "Add an unguarded pattern for the missing variant."),
            "TP5002" => ("Pending outcome abandoned", "The final local carrier of a persistent outcome is lost while acknowledgment is pending.", "Acknowledge reachable alternatives or record an intentional discard.", "An opted-in producer delivered a resolved CodePact outcome.", "Unknown external carriers retain incomplete coverage.", "discard_outcome(value, reason='redundant response')", "del value", "the outcome may have escaped to an unknown helper", "Acknowledge pending alternatives before the final carrier is lost."),
            "TP5003" => ("Invalid persistent outcome declaration or waiver", "A MustHandle declaration or discard_outcome waiver is outside the supported form.", "Use an exact Result or Option callable return and a nonempty literal discard reason.", "The declaration or waiver resolves to CodePact.", "Unsupported annotations retain incomplete coverage.", "def fetch() -> MustHandle[Option[str]]: ...", "field: MustHandle[Option[str]]", "the annotation name may be shadowed", "Correct the declaration or waiver."),
            "TP5004" => ("Undeclared outcome handoff", "A newly acquired pending outcome leaves a callable without an outward MustHandle agreement.", "Declare the outward agreement or acknowledge the outcome before returning.", "A pending producer identity reaches the actual return.", "Unsupported carriers retain incomplete coverage.", "def wrapper() -> MustHandle[Option[str]]: return fetch()", "def wrapper() -> Option[str]: return fetch()", "the returned value came through unresolved storage", "Declare the agreement or handle the outcome before return."),
            "TP5005" => ("Intentional outcome discard", "A resolved persistent outcome was discarded with a recorded reason.", "Review the recorded reason under project policy.", "The literal reason and affected outcome resolve at the waiver site.", "The waiver supplies no resource cleanup or work observation effect.", "discard_outcome(value, reason='redundant response')", "discard_outcome(value, reason='')", "the reason is computed dynamically", "Retain the reason in review evidence."),
            "TP6001" => ("Resource declaration hint", "A local class with a supported close method is passed to contextlib.closing.", "Consider declaring the class with @resource(closed_by=\"close\").", "The class and closing call resolve to local source and an unshadowed contextlib identity.", "This is a review hint only; it neither verifies close behavior nor changes analysis coverage or policy.", "@resource(closed_by=\"close\")\\nclass Connection: ...", "with closing(Connection()): ...", "the class or closing callable is unresolved or rebound", "Review the close method and add the resource declaration if it describes the intended lifecycle."),
            "TP6002" => ("Must-use declaration hint", "An unmarked synchronous wrapper directly returns a resolved must-use result.", "Consider marking the wrapper with @must_use.", "The returned call resolves to a supported synchronous must-use declaration.", "This is a review hint only; callers are not checked until the wrapper is marked.", "@must_use\\ndef fetch(): ...\\ndef wrapper(): return fetch()", "def wrapper(): return fetch()", "the call or wrapper binding is unresolved, rebound or shadowed", "Review the wrapper contract and mark it if callers should handle its result."),
            "TP6003" => ("Singleton wildcard acknowledgment", "A wildcard acknowledged the sole remaining variant of a pending outcome.", "Review the variant and related producer site.", "CodePact established one reachable alternative when the guarded arm was entered.", "A wildcard covering multiple variants or an unentered arm supplies no acknowledgment.", "match outcome:\\n    case Err(_): pass\\n    case _: pass", "match outcome:\\n    case _: pass", "the remaining variant or carrier is unresolved", "Name a variant explicitly when the wildcard covers several alternatives."),
            "TP4001" => ("Python syntax error", "The source could not be parsed for its selected Python target.", "Correct the syntax or select the target that matches the source.", "The parser receives source bytes and a supported Python target.", "This diagnostic does not claim analysis of syntax the selected parser could not build.", "def f(): pass", "def f(: pass", "the selected Python target may not match the source grammar", "Correct the source syntax or change the declared Python target."),
            "TP4002" => ("Invalid declaration or policy", "A declaration or inline policy directive is invalid.", "Correct the exact declaration or directive shown in the report.", "A known CodePact declaration or policy form is recognized.", "Ordinary Python type checking is outside this diagnostic.", "# codepact: ignore[TP3001] -- intentional discard", "# codepact: ignore[TP9001] -- hide boundary", "an unknown third-party annotation may not be interpreted", "Correct the metadata or use an exact eligible suppression code and reason."),
            "TP4101" => ("Missing resource access declaration", "A selected synchronous signature has a known resource parameter with unspecified access.", "Declare access according to the function body and caller responsibilities.", "The project opts into resource-access policy and the resource resolves.", "Unresolved resource annotations retain their coverage boundary.", "def get(connection: Borrowed[Connection]): ...", "def get(connection: Connection): ...", "the parameter resource is unresolved", "Declare Owned, Borrowed, or BorrowedMut."),
            "TP4102" => ("Missing owned return declaration", "A selected synchronous API requires an owned resource handoff.", "Review the handoff and declare the owned resource return.", "The project opts into owned-return policy for the selected signature.", "Body checking determines whether the claimed transfer is supported.", "def open_connection() -> Owned[Connection]: ...", "def open_connection() -> Connection: ...", "the selected function is an unsupported generator", "Declare the resource return Owned if the caller receives cleanup responsibility."),
            "TP4103" => ("Missing immediate-use declaration", "A selected synchronous API requires an immediate-use declaration.", "Mark the selected return when its result requires immediate use.", "The project opts into must-use policy for the selected signature.", "Persistent outcome acknowledgment has an independent contract.", "@must_use\ndef fetch(): ...", "def fetch(): ...", "a decorator obscures the selected function", "Declare @must_use or annotate the return with MustUse."),
            "TP9001" => ("Analysis boundary", "A construct or effect is outside the qualified analysis subset.", "Review the unsupported construct; a clean supported-subset result is not a general safety proof.", "The source is valid enough for the frontend to identify the construct.", "This is not a confirmed violation; boundary reporting never proves safety or execution behavior.", "simple local open/use/close", "dynamic dispatch treated as a proven close", "task ownership or framework behavior outside the model", "Resolve the boundary or record it as a limitation; do not change policy to call it repaired."),
            "TP9002" => ("Analysis cancelled", "The requested analysis did not complete.", "Retry the check; do not treat this result as a clean report.", "The analyzer request was cancelled before a completed report was produced.", "Cancellation of analyzed Python code is distinct from cancellation of the analyzer request.", "completed check", "partial report treated as complete", "the caller cancels while work remains", "Retry and verify a completed report with the current snapshot."),
            "TP7001" => ("Asyncio queue protocol", "A queue operation violates the local accounting or shutdown protocol.", "Pair each successful receive with its own task_done on every path.", "The queue and operation resolve to the selected asyncio model.", "Queue accounting, delivery and completion have independent effects.", "item = await queue.get()\ntry: use(item)\nfinally: queue.task_done()", "item = await queue.get()\nuse(item)", "the queue or consumer is unresolved", "Review cancellation and account for each received item."),
            "TP7002" => ("Asyncio synchronization protocol", "A guard, permit or signal protocol is violated.", "Acquire the required lock before accessing the guarded operation.", "The guard belongs to the selected asyncio model.", "The check covers the modeled scope and acquisition identity.", "async with lock: resource.update()", "resource.update()", "the guard or effect is unresolved", "Use the exact lock with a supported acquisition and release pair."),
            "TP7003" => ("Multiprocessing IPC protocol", "A queue or connection protocol is violated.", "Check the endpoint lifecycle, accounting and feeder cleanup.", "The primitive resolves to the selected multiprocessing model.", "Admission, receipt, accounting and cleanup have independent evidence.", "queue.close()\nqueue.join_thread()", "queue.join_thread()", "the peer copy or delivery is unresolved", "Follow the cleanup order for the specific primitive."),
            "TP7004" => ("Multiprocessing synchronization protocol", "An acquisition, release or shared update violates the selected process protocol.", "Hold the exact recursive wrapper lock for the shared update.", "The primitive resolves to the reviewed synchronization model.", "Synchronized properties guard their individual accesses.", "with value.get_lock(): value.value += 1", "value.value += 1", "the wrapper or guard is unresolved", "Acquire the modeled guard and unwind each established acquisition."),
            "TP7005" => ("Multiprocessing spawn entry", "A direct-script launch violates the reviewed spawn entry protocol.", "Use a main guard, an importable top-level worker and applicable arguments.", "The direct script and explicit spawn context match the analyzed runtime.", "Start and child bootstrap are separate effects.", "if __name__ == '__main__': process.start()", "process.start()", "the entry mode or worker is unresolved", "Review the launch evidence and worker arguments."),
            _ => return None,
        };
    let id = format!("{rule}.R001");
    let output = serde_json::to_string_pretty(&serde_json::json!({
            "explanation_schema_version":1,"explanation_id":id,"code":rule,
            "title":explanation.0,"summary":explanation.1,
            "preconditions":[explanation.3],"limitations":[explanation.4],
            "examples":{"valid":explanation.5,"invalid":explanation.6,"unknown":explanation.7},
            "repair_guidance":{"id":format!("{rule}.repair.R001"),"text":explanation.2,"suggested_action":explanation.8},
            "automatic_fix":false
        })).unwrap();
    Some(output)
}
