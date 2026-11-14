use super::*;

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
            {"code":"TP4001","semantic_version":1,"tier":"mandatory","enabled_by_default":true,"explanation_id":"TP4001.R001"},
            {"code":"TP4002","semantic_version":1,"tier":"mandatory","enabled_by_default":true,"explanation_id":"TP4002.R001"},
            {"code":"TP9001","semantic_version":1,"tier":"mandatory","enabled_by_default":true,"explanation_id":"TP9001.R001"},
            {"code":"TP9002","semantic_version":1,"tier":"mandatory","enabled_by_default":true,"explanation_id":"TP9002.R001"}
        ],
        "formats": ["text", "json", "concise"],
        "settings": ["root", "python-version", "include", "exclude", "enabled-rules", "mode", "fail-on", "baseline", "severity", "experimental-ownership", "assume-default-task-factory"],
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
            "TP3001" => ("Discarded must-use result", "A declared must-use result is discarded as an expression statement.", "Handle the value, assign it for immediate handling, or pass it to codepact.discard when abandonment is intentional.", "The synchronous callable resolves to a supported must-use declaration.", "Assignment is immediate handling only; later use or eventual consumption is not proven. Async and dynamic consumption are unqualified.", "value = fetch()", "fetch()", "the callee is dynamically selected or the result is nested in an unsupported expression", "Handle the result, or use codepact.discard(call()) when intentional discard is explicit."),
            "TP4001" => ("Python syntax error", "The source could not be parsed for its selected Python target.", "Correct the syntax or select the target that matches the source.", "The parser receives source bytes and a supported Python target.", "This diagnostic does not claim analysis of syntax the selected parser could not build.", "def f(): pass", "def f(: pass", "the selected Python target may not match the source grammar", "Correct the source syntax or change the declared Python target."),
            "TP4002" => ("Invalid declaration or policy", "A declaration or inline policy directive is invalid.", "Correct the exact declaration or directive shown in the report.", "A known CodePact declaration or policy form is recognized.", "Ordinary Python type checking is outside this diagnostic.", "# codepact: ignore[TP3001] -- intentional discard", "# codepact: ignore[TP9001] -- hide boundary", "an unknown third-party annotation may not be interpreted", "Correct the metadata or use an exact eligible suppression code and reason."),
            "TP9001" => ("Analysis boundary", "A construct or effect is outside the qualified analysis subset.", "Review the unsupported construct; a clean supported-subset result is not a general safety proof.", "The source is valid enough for the frontend to identify the construct.", "This is not a confirmed violation; boundary reporting never proves safety or execution behavior.", "simple local open/use/close", "dynamic dispatch treated as a proven close", "task ownership or framework behavior outside the model", "Resolve the boundary or record it as a limitation; do not change policy to call it repaired."),
            "TP9002" => ("Analysis cancelled", "The requested analysis did not complete.", "Retry the check; do not treat this result as a clean report.", "The analyzer request was cancelled before a completed report was produced.", "Cancellation of analyzed Python code is distinct from cancellation of the analyzer request.", "completed check", "partial report treated as complete", "the caller cancels while work remains", "Retry and verify a completed report with the current snapshot."),
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
