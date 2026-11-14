use codepact::{check_with_ownership, CheckResult, Ownership, SourceFile};

const HEADER: &str = "from typing import Literal\nfrom codepact import resource, state_machine, transition\nfrom codepact.rust import Borrowed, BorrowedMut, State\n@resource(closed_by='close')\nclass C:\n    def close(self): pass\n    def read(self): pass\n    def write(self): pass\n";

fn check(body: &str, ownership: Ownership, python_version: &str) -> CheckResult {
    let files = [SourceFile::new(
        "main.py",
        "main",
        format!("{HEADER}{body}"),
    )];
    check_with_ownership(&files, python_version, ownership)
}

fn codes(result: &CheckResult) -> Vec<&str> {
    result
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect()
}

fn has(result: &CheckResult, code: &str) {
    assert!(
        codes(result).contains(&code),
        "{code} missing from {result:#?}"
    );
}

fn lacks(result: &CheckResult, code: &str) {
    assert!(
        !codes(result).contains(&code),
        "{code} unexpected in {result:#?}"
    );
}

#[test]
fn v06_01_default_and_ownership_scopes_exclude_advanced_rules() {
    let files = [SourceFile::new(
        "main.py",
        "main",
        format!(
            "{HEADER}def edit(value: BorrowedMut[C], alias: Borrowed[C]):\n    value.write()\n    alias.read()\ndef run():\n    item = C()\n    alias = item\n    edit(item, alias)\n"
        ),
    )];
    for ownership in [Ownership::Disabled, Ownership::Enabled] {
        let result = check_with_ownership(&files, "3.13", ownership);
        lacks(&result, "TP2101");
        lacks(&result, "TP2102");
    }
    let advanced = check_with_ownership(&files, "3.13", Ownership::Advanced);
    has(&advanced, "TP2101");
}

#[test]
fn v06_02_state_transition_and_receiver_precondition_share_state() {
    let result = check(
        "@state_machine(states=('ready', 'closed'), initial='ready')\nclass Channel:\n    @transition(from_state='ready', to_state='closed', on_raise='ready')\n    def close(self): pass\n    def send(self: State['Channel', Literal['ready']]): pass\ndef run():\n    channel = Channel()\n    channel.close()\n    channel.send()\n",
        Ownership::Advanced,
        "3.13",
    );
    has(&result, "TP2102");
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
}

#[test]
fn v06_02_receiver_precondition_accepts_initial_state() {
    let result = check(
        "@state_machine(states=('ready', 'closed'), initial='ready')\nclass Channel:\n    def send(self: State['Channel', Literal['ready']]): pass\ndef run():\n    channel = Channel()\n    channel.send()\n",
        Ownership::Advanced,
        "3.13",
    );
    lacks(&result, "TP2102");
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
}

#[test]
fn v06_02_annotated_state_access_matches_state_alias() {
    let alias = check(
        "@state_machine(states=('ready', 'closed'), initial='ready')\nclass Channel:\n    @transition(from_state='ready', to_state='closed')\n    def close(self): pass\n    def send(self: State['Channel', Literal['ready']]): pass\ndef run():\n    channel = Channel()\n    channel.close()\n    channel.send()\n",
        Ownership::Advanced,
        "3.13",
    );
    let explicit = check(
        "from typing import Annotated\nfrom codepact.contracts import StateAccess\n@state_machine(states=('ready', 'closed'), initial='ready')\nclass Channel:\n    @transition(from_state='ready', to_state='closed')\n    def close(self): pass\n    def send(self: Annotated['Channel', StateAccess[Literal['ready']]]): pass\ndef run():\n    channel = Channel()\n    channel.close()\n    channel.send()\n",
        Ownership::Advanced,
        "3.13",
    );
    let alias_finding = alias
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "TP2102")
        .expect("State alias precondition is enforced");
    let explicit_finding = explicit
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "TP2102")
        .expect("Annotated StateAccess precondition is enforced");
    assert_eq!(alias_finding.certainty, explicit_finding.certainty);
    assert_eq!(alias_finding.trust, explicit_finding.trust);
    assert!(
        alias
            .coverage
            .reasons
            .iter()
            .all(|reason| !reason.contains("method parameter annotations")),
        "{alias:#?}"
    );
    assert!(
        explicit
            .coverage
            .reasons
            .iter()
            .all(|reason| !reason.contains("method parameter annotations")),
        "{explicit:#?}"
    );
}

#[test]
fn v06_03_identical_transition_declarations_normalize_and_conflicts_are_rejected() {
    let duplicate = check(
        "@state_machine(states=('ready', 'closed'), initial='ready')\nclass Channel:\n    @transition(from_state='ready', to_state='closed')\n    @transition(from_state='ready', to_state='closed')\n    def close(self): pass\n    def send(self: State['Channel', Literal['ready']]): pass\ndef run():\n    channel = Channel()\n    channel.close()\n    channel.send()\n",
        Ownership::Advanced,
        "3.13",
    );
    lacks(&duplicate, "TP4002");
    has(&duplicate, "TP2102");

    let conflict = check(
        "@state_machine(states=('ready', 'closed'), initial='ready')\nclass Channel:\n    @transition(from_state='ready', to_state='closed')\n    @transition(from_state='ready', to_state='ready')\n    def close(self): pass\ndef run():\n    Channel().close()\n",
        Ownership::Advanced,
        "3.13",
    );
    has(&conflict, "TP4002");
    lacks(&conflict, "TP2102");
}

#[test]
fn v06_03_transition_to_an_undeclared_state_is_rejected() {
    let result = check(
        "@state_machine(states=('ready', 'closed'), initial='ready')\nclass Channel:\n    @transition(from_state='ready', to_state='missing')\n    def close(self): pass\ndef run():\n    Channel().close()\n",
        Ownership::Advanced,
        "3.13",
    );
    has(&result, "TP4002");
    lacks(&result, "TP2102");
}

#[test]
fn v06_02_import_aliases_resolve_protocol_declarations() {
    let result = check(
        "from codepact import state_machine as machine, transition as changes\n@machine(states=('ready', 'closed'), initial='ready')\nclass Channel:\n    @changes(from_state='ready', to_state='closed')\n    def close(self): pass\n    def send(self: State['Channel', Literal['ready']]): pass\ndef run():\n    channel = Channel()\n    channel.close()\n    channel.send()\n",
        Ownership::Advanced,
        "3.13",
    );
    has(&result, "TP2102");
    assert!(
        result
            .coverage
            .reasons
            .iter()
            .all(|reason| !reason.contains("unsupported flow")),
        "{result:#?}"
    );
}

#[test]
fn v06_02_protocol_contract_survives_qualified_named_reexports() {
    let files = [
        SourceFile::new(
            "provider.py",
            "provider",
            "from typing import Literal\nfrom codepact import state_machine, transition\nfrom codepact.rust import State\n@state_machine(states=('ready', 'closed'), initial='ready')\nclass Channel:\n    @transition(from_state='ready', to_state='closed')\n    def close(self): pass\n    def send(self: State['Channel', Literal['ready']]): pass\n",
        ),
        SourceFile::new(
            "reexports.py",
            "reexports",
            "from provider import Channel as PublicChannel\n",
        ),
        SourceFile::new(
            "main.py",
            "main",
            "from reexports import PublicChannel\ndef run():\n    channel = PublicChannel()\n    channel.close()\n    channel.send()\n",
        ),
    ];
    let result = check_with_ownership(&files, "3.13", Ownership::Advanced);
    let finding = result
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "TP2102")
        .expect("the re-exported transition is applied to the caller instance");
    assert_eq!(finding.primary.file, "main.py", "{result:#?}");
    assert_eq!(finding.related[0].file, "provider.py", "{result:#?}");
}

#[test]
fn v06_02_local_lookalikes_do_not_create_codepact_protocols() {
    let result = check(
        "from typing import Literal\nfrom codepact.rust import State\ndef state_machine(**kwargs):\n    return lambda target: target\ndef transition(**kwargs):\n    return lambda function: function\n@state_machine(states=('ready', 'closed'), initial='ready')\nclass Channel:\n    @transition(from_state='ready', to_state='closed')\n    def close(self): pass\n    def send(self: State['Channel', Literal['ready']]): pass\ndef run():\n    channel = Channel()\n    channel.close()\n    channel.send()\n",
        Ownership::Advanced,
        "3.13",
    );
    lacks(&result, "TP2102");
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
}

#[test]
fn v06_03_invalid_state_machine_is_rejected_before_state_is_seeded() {
    let result = check(
        "@state_machine(states=('ready',), initial='missing')\nclass Channel:\n    def send(self: State['Channel', Literal['ready']]): pass\ndef run():\n    Channel().send()\n",
        Ownership::Advanced,
        "3.13",
    );
    has(&result, "TP4002");
    lacks(&result, "TP2102");
}

#[test]
fn v06_03_nonliteral_state_metadata_is_rejected_before_flow() {
    let result = check(
        "STATES = ('ready', 'closed')\n@state_machine(states=STATES, initial='ready')\nclass Channel:\n    @transition(from_state='ready', to_state='closed')\n    def close(self): pass\ndef run():\n    Channel().close()\n",
        Ownership::Advanced,
        "3.13",
    );
    has(&result, "TP4002");
    lacks(&result, "TP2102");
}

#[test]
fn v06_04_same_object_shared_and_exclusive_arguments_conflict() {
    let result = check(
        "def edit(value: BorrowedMut[C], observer: Borrowed[C]):\n    value.write()\n    observer.read()\ndef run():\n    item = C()\n    edit(item, item)\n",
        Ownership::Advanced,
        "3.13",
    );
    has(&result, "TP2101");
}

#[test]
fn v06_04_same_object_exclusive_arguments_conflict() {
    let result = check(
        "def edit(first: BorrowedMut[C], second: BorrowedMut[C]):\n    first.write()\n    second.write()\ndef run():\n    item = C()\n    edit(item, item)\n",
        Ownership::Advanced,
        "3.13",
    );
    has(&result, "TP2101");
}

#[test]
fn v06_04_distinct_objects_and_sequential_exclusive_calls_are_valid() {
    let result = check(
        "def edit(value: BorrowedMut[C]):\n    value.write()\ndef run():\n    first = C()\n    second = C()\n    edit(first)\n    edit(second)\n",
        Ownership::Advanced,
        "3.13",
    );
    lacks(&result, "TP2101");
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
}

#[test]
fn v06_04_local_alias_of_exclusive_view_is_allowed() {
    let result = check(
        "def edit(value: BorrowedMut[C]):\n    view = value\n    view.write()\ndef run():\n    item = C()\n    edit(item)\n",
        Ownership::Advanced,
        "3.13",
    );
    lacks(&result, "TP2101");
}

#[test]
fn v06_05_competing_owner_alias_access_is_reported_inside_region() {
    let result = check(
        "def edit(value: BorrowedMut[C], alias: C):\n    value.write()\n    alias.write()\ndef run():\n    item = C()\n    edit(item, item)\n",
        Ownership::Advanced,
        "3.13",
    );
    has(&result, "TP2101");
}

#[test]
fn v06_05_exclusive_region_ends_after_a_propagated_raise() {
    let result = check(
        "def edit(value: BorrowedMut[C]):\n    value.write()\n    raise ValueError()\ndef run():\n    item = C()\n    try:\n        edit(item)\n    except ValueError:\n        edit(item)\n    item.close()\n",
        Ownership::Advanced,
        "3.13",
    );
    lacks(&result, "TP2101");
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
}

#[test]
fn v06_04_conflict_reports_the_access_and_region_origin_spans() {
    let result = check(
        "def edit(value: BorrowedMut[C], alias: C):\n    value.write()\n    alias.read()\ndef run():\n    item = C()\n    edit(item, item)\n",
        Ownership::Advanced,
        "3.13",
    );
    let finding = result
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "TP2101")
        .expect("the competing alias access is rejected");
    assert_eq!(finding.primary.file, "main.py");
    assert_eq!(finding.primary.line, 11, "{result:#?}");
    assert_eq!(finding.related.len(), 1, "{result:#?}");
    assert_eq!(finding.related[0].file, "main.py");
    assert_eq!(finding.related[0].line, 9, "{result:#?}");
}

#[test]
fn v06_04_live_task_capture_conflicts_with_exclusive_call() {
    let files = [SourceFile::new(
        "main.py",
        "main",
        format!(
            "from asyncio import TaskGroup\n{HEADER}async def reader(value: Borrowed[C]):\n    value.read()\ndef edit(value: BorrowedMut[C]):\n    value.write()\nasync def run():\n    item = C()\n    async with TaskGroup() as group:\n        group.create_task(reader(item))\n        edit(item)\n"
        ),
    )];
    let result = check_with_ownership(&files, "3.13", Ownership::AdvancedAssumeDefaultTaskFactory);
    let finding = result
        .diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.code == "TP2101" && diagnostic.message.contains("live task capture")
        })
        .expect("exclusive access overlaps the submitted shared capture");
    assert_eq!(finding.certainty, "definite", "{result:#?}");
    assert_eq!(finding.primary.line, 18, "{result:#?}");
    assert_eq!(finding.related.len(), 1, "{result:#?}");
    assert_eq!(finding.related[0].line, 17, "{result:#?}");
}

#[test]
fn v06_06_pending_exclusive_coroutine_remains_a_visible_capture_boundary() {
    let result = check(
        "async def edit(value: BorrowedMut[C]):\n    value.write()\ndef run():\n    item = C()\n    pending = edit(item)\n    edit(item)\n",
        Ownership::Advanced,
        "3.13",
    );
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert!(
        result.coverage.reasons.iter().any(|reason| {
            reason.contains("exclusive borrow is captured by an unawaited coroutine")
                || reason.contains("unresolved coroutine capture")
        }),
        "{result:#?}"
    );
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TP9001"
                && diagnostic.message.contains("exclusive borrow is captured")
        }),
        "{result:#?}"
    );
}

#[test]
fn v06_06_await_inside_exclusive_region_is_a_visible_boundary() {
    let result = check(
        "async def edit(value: BorrowedMut[C], pending):\n    await pending\n    value.write()\n",
        Ownership::Advanced,
        "3.13",
    );
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert!(
        result
            .coverage
            .reasons
            .iter()
            .any(|reason| reason.contains("exclusive borrow remains active across await")),
        "{result:#?}"
    );
}

#[test]
fn v06_06_implicit_async_and_generator_suspension_keep_exclusive_boundary_visible() {
    let async_context = check(
        "class Manager:\n    async def __aenter__(self): return self\n    async def __aexit__(self, *args): return False\nasync def edit(value: BorrowedMut[C], manager: Manager):\n    async with manager:\n        value.write()\n",
        Ownership::Advanced,
        "3.13",
    );
    assert_eq!(
        async_context.coverage.status, "incomplete",
        "{async_context:#?}"
    );
    assert!(
        async_context.coverage.reasons.iter().any(|reason| {
            reason.contains("exclusive borrow remains active across async context entry")
                || reason.contains("exclusive borrow remains active across async context exit")
        }),
        "{async_context:#?}"
    );

    let generator = check(
        "def edit(value: BorrowedMut[C]):\n    yield 1\n    value.write()\n",
        Ownership::Advanced,
        "3.13",
    );
    assert_eq!(generator.coverage.status, "incomplete", "{generator:#?}");
    assert!(
        generator
            .coverage
            .reasons
            .iter()
            .any(|reason| reason.contains("exclusive borrow remains active across yield")),
        "{generator:#?}"
    );
}

#[test]
fn v06_07_branch_join_keeps_possible_protocol_violation() {
    let result = check(
        "@state_machine(states=('ready', 'closed'), initial='ready')\nclass Channel:\n    @transition(from_state=('ready', 'closed'), to_state='closed')\n    def close(self): pass\n    def send(self: State['Channel', Literal['ready']]): pass\ndef run(flag):\n    channel = Channel()\n    if flag:\n        channel.close()\n    channel.send()\n",
        Ownership::Advanced,
        "3.13",
    );
    let finding = result
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "TP2102")
        .expect("joined closed path violates the precondition");
    assert_eq!(finding.certainty, "possible", "{result:#?}");
}

#[test]
fn v06_07_unreachable_transition_does_not_pollute_join_but_unknown_effect_does() {
    let unreachable = check(
        "@state_machine(states=('ready', 'closed'), initial='ready')\nclass Channel:\n    @transition(from_state=('ready', 'closed'), to_state='closed')\n    def close(self): pass\n    def send(self: State['Channel', Literal['ready']]): pass\ndef run():\n    channel = Channel()\n    if False:\n        channel.close()\n    channel.send()\n",
        Ownership::Advanced,
        "3.13",
    );
    lacks(&unreachable, "TP2102");

    let unknown = check(
        "@state_machine(states=('ready', 'closed'), initial='ready')\nclass Channel:\n    def send(self: State['Channel', Literal['ready']]): pass\ndef run(channel):\n    unknown_effect(channel)\n    channel.send()\n",
        Ownership::Advanced,
        "3.13",
    );
    assert_eq!(unknown.coverage.status, "incomplete", "{unknown:#?}");
    assert!(
        unknown
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "TP2102")
            .all(|diagnostic| diagnostic.certainty != "definite"),
        "{unknown:#?}"
    );
}

#[test]
fn v06_07_known_violation_survives_an_unresolved_alternative_as_possible() {
    let result = check(
        "@state_machine(states=('ready', 'closed'), initial='ready')\nclass Channel:\n    @transition(from_state=('ready', 'closed'), to_state='closed')\n    def close(self): pass\n    def send(self: State['Channel', Literal['ready']]): pass\ndef run(flag):\n    channel = Channel()\n    if flag:\n        channel.close()\n    else:\n        unknown_effect(channel)\n    channel.send()\n",
        Ownership::Advanced,
        "3.13",
    );
    let finding = result
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "TP2102")
        .expect("the known violating branch remains reported");
    assert_eq!(finding.certainty, "possible", "{result:#?}");
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
}

#[test]
fn v06_08_omitted_exception_transition_widens_only_exceptional_state() {
    let result = check(
        "@state_machine(states=('ready', 'closed'), initial='ready')\nclass Channel:\n    @transition(from_state='ready', to_state='closed')\n    def close(self):\n        if True: raise ValueError()\ndef run():\n    channel = Channel()\n    try:\n        channel.close()\n    except ValueError:\n        channel.send()\n",
        Ownership::Advanced,
        "3.13",
    );
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
}

#[test]
fn v06_08_explicit_exceptional_transition_applies_only_to_the_raise_edge() {
    let result = check(
        "@state_machine(states=('ready', 'closed'), initial='ready')\nclass Channel:\n    @transition(from_state='ready', to_state='closed', on_raise='ready')\n    def close(self):\n        raise ValueError()\n    def send(self: State['Channel', Literal['ready']]): pass\ndef run():\n    channel = Channel()\n    try:\n        channel.close()\n    except ValueError:\n        channel.send()\n",
        Ownership::Advanced,
        "3.13",
    );
    lacks(&result, "TP2102");
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
}

#[test]
fn v06_08_explicit_cancellation_transition_applies_to_cancelled_await_edge() {
    let result = check(
        "@state_machine(states=('ready', 'closed'), initial='ready')\nclass Channel:\n    @transition(from_state='ready', to_state='closed', on_raise='ready', on_cancel='ready')\n    async def close(self):\n        await unknown_wait()\n    def send(self: State['Channel', Literal['ready']]): pass\nasync def run():\n    channel = Channel()\n    try:\n        await channel.close()\n    except BaseException:\n        channel.send()\n",
        Ownership::Advanced,
        "3.13",
    );
    lacks(&result, "TP2102");
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert!(
        result.coverage.reasons.iter().any(|reason| {
            reason.contains("cancellation may interrupt the awaited operation or its cleanup")
        }),
        "{result:#?}"
    );
}

#[test]
fn v06_09_declared_transition_is_unverified_and_unresolved_effects_stay_unknown() {
    let declared = check(
        "@state_machine(states=('ready', 'closed'), initial='ready')\nclass Channel:\n    @transition(from_state='ready', to_state='closed')\n    def close(self): pass\n    def send(self: State['Channel', Literal['ready']]): pass\ndef run():\n    channel = Channel()\n    channel.close()\n    channel.send()\n",
        Ownership::Advanced,
        "3.13",
    );
    has(&declared, "TP2102");
    assert!(
        declared.contracts.iter().any(|contract| contract
            .provenance
            .iter()
            .any(|item| item.trust == "declared-unverified")),
        "{declared:#?}"
    );

    let unresolved = check(
        "@state_machine(states=('ready', 'closed'), initial='ready')\nclass Channel:\n    def send(self: State['Channel', Literal['ready']]): pass\ndef run(channel):\n    unknown_effect(channel)\n    channel.send()\n",
        Ownership::Advanced,
        "3.13",
    );
    assert_eq!(unresolved.coverage.status, "incomplete", "{unresolved:#?}");
}

#[test]
fn v06_09_declared_exception_postcondition_reestablishes_protocol_state() {
    let result = check(
        "@state_machine(states=('ready', 'closed'), initial='ready')\nclass Channel:\n    @transition(from_state='ready', to_state='closed', on_raise='ready')\n    def close(self):\n        unknown_effect(self)\n        raise ValueError()\n    def send(self: State['Channel', Literal['ready']]): pass\ndef run():\n    channel = Channel()\n    try:\n        channel.close()\n    except ValueError:\n        channel.send()\n",
        Ownership::Advanced,
        "3.13",
    );
    lacks(&result, "TP2102");
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
}

#[test]
fn v06_10_recursive_protocol_calls_terminate_with_conservative_state() {
    let result = check(
        "@state_machine(states=('ready', 'closed'), initial='ready')\nclass Channel:\n    @transition(from_state='ready', to_state='closed')\n    def close(self, again):\n        if again:\n            self.close(False)\n    def send(self: State['Channel', Literal['ready']]): pass\ndef run(flag):\n    channel = Channel()\n    channel.close(flag)\n    channel.send()\n",
        Ownership::Advanced,
        "3.13",
    );
    let finding = result
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "TP2102")
        .expect("the declared outer completion state remains visible after recursive widening");
    assert_eq!(finding.certainty, "definite", "{result:#?}");
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert!(
        result
            .coverage
            .reasons
            .iter()
            .any(|reason| reason.contains("recursive method summary is unknown")),
        "{result:#?}"
    );
}

#[test]
fn v06_11_local_task_group_after_exit_is_invalid() {
    let files = [SourceFile::new(
        "main.py",
        "main",
        "import asyncio\nasync def worker(): pass\nasync def run():\n    group = asyncio.TaskGroup()\n    async with group:\n        group.create_task(worker())\n    group.create_task(worker())\n",
    )];
    let result = check_with_ownership(&files, "3.13", Ownership::AdvancedAssumeDefaultTaskFactory);
    has(&result, "TP2102");
}

#[test]
fn v06_11_local_task_group_before_entry_and_reentry_are_invalid() {
    let before_entry = [SourceFile::new(
        "main.py",
        "main",
        "import asyncio\nasync def worker(): pass\nasync def run():\n    group = asyncio.TaskGroup()\n    group.create_task(worker())\n",
    )];
    let result = check_with_ownership(
        &before_entry,
        "3.13",
        Ownership::AdvancedAssumeDefaultTaskFactory,
    );
    has(&result, "TP2102");

    let reentry = [SourceFile::new(
        "main.py",
        "main",
        "import asyncio\nasync def run():\n    group = asyncio.TaskGroup()\n    async with group: pass\n    async with group: pass\n",
    )];
    let result = check_with_ownership(
        &reentry,
        "3.13",
        Ownership::AdvancedAssumeDefaultTaskFactory,
    );
    has(&result, "TP2102");
}

#[test]
fn v06_11_nested_local_task_groups_keep_each_group_scope_valid() {
    let files = [SourceFile::new(
        "main.py",
        "main",
        "import asyncio\nasync def worker(): pass\nasync def run():\n    async with asyncio.TaskGroup() as outer:\n        async with asyncio.TaskGroup() as inner:\n            inner.create_task(worker())\n        outer.create_task(worker())\n",
    )];
    let result = check_with_ownership(&files, "3.13", Ownership::AdvancedAssumeDefaultTaskFactory);
    lacks(&result, "TP2102");
}

#[test]
fn v06_12_executor_submission_after_shutdown_is_invalid_and_pinned() {
    let files = [SourceFile::new(
        "main.py",
        "main",
        "from concurrent.futures import ThreadPoolExecutor\ndef worker(): return 1\ndef run():\n    pool = ThreadPoolExecutor()\n    pool.shutdown()\n    pool.submit(worker)\n",
    )];
    let result = check_with_ownership(&files, "3.13", Ownership::Advanced);
    has(&result, "TP2102");
    assert!(
        result
            .available_models
            .iter()
            .any(|model| model.id == "TPM-EXECUTOR-001" && model.source_revisions == ["v3.13.16"]),
        "{result:#?}"
    );
}

#[test]
fn v06_12_executor_accepts_submit_and_repeated_shutdown() {
    let files = [SourceFile::new(
        "main.py",
        "main",
        "from concurrent.futures import ThreadPoolExecutor\ndef worker(): return 1\ndef run():\n    pool = ThreadPoolExecutor()\n    pool.submit(worker)\n    pool.shutdown()\n    pool.shutdown()\n",
    )];
    let result = check_with_ownership(&files, "3.13", Ownership::Advanced);
    lacks(&result, "TP2102");
    assert_eq!(
        result.coverage.status, "complete-for-supported-subset",
        "{result:#?}"
    );
}

#[test]
fn v06_12_nonliteral_executor_configuration_remains_a_boundary() {
    let files = [SourceFile::new(
        "main.py",
        "main",
        "from concurrent.futures import ThreadPoolExecutor\ndef worker(): return 1\ndef run(workers):\n    pool = ThreadPoolExecutor(max_workers=workers)\n    pool.submit(worker)\n",
    )];
    let result = check_with_ownership(&files, "3.13", Ownership::Advanced);
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert!(
        result
            .available_models
            .iter()
            .any(|model| model.id == "TPM-EXECUTOR-001"),
        "{result:#?}"
    );
}

#[test]
fn v06_12_executor_submission_with_tracked_resource_arguments_is_a_boundary() {
    let files = [SourceFile::new(
        "main.py",
        "main",
        format!(
            "from concurrent.futures import ThreadPoolExecutor\n{HEADER}def worker(value: Borrowed[C]):\n    value.read()\ndef run():\n    item = C()\n    pool = ThreadPoolExecutor()\n    pool.submit(worker, item)\n    pool.shutdown()\n    item.close()\n"
        ),
    )];
    let result = check_with_ownership(&files, "3.13", Ownership::Advanced);
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
    assert!(
        result.coverage.reasons.iter().any(|reason| {
            reason.contains("ThreadPoolExecutor")
                || reason.contains("executor")
                || reason.contains("worker")
        }),
        "{result:#?}"
    );
}

#[test]
fn v06_12_executor_model_is_not_selected_for_other_target_versions() {
    let files = [SourceFile::new(
        "main.py",
        "main",
        "from concurrent.futures import ThreadPoolExecutor\ndef worker(): return 1\ndef run():\n    pool = ThreadPoolExecutor()\n    pool.submit(worker)\n",
    )];
    let result = check_with_ownership(&files, "3.12", Ownership::Advanced);
    assert!(
        result
            .available_models
            .iter()
            .all(|model| model.id != "TPM-EXECUTOR-001"),
        "{result:#?}"
    );
    assert_eq!(result.coverage.status, "incomplete", "{result:#?}");
}

#[test]
fn v06_13_advanced_session_matches_fresh_results_after_contract_edits() {
    let original = SourceFile::new(
        "main.py",
        "main",
        format!(
            "{HEADER}@state_machine(states=('ready', 'closed'), initial='ready')\nclass Channel:\n    @transition(from_state='ready', to_state='closed')\n    def close(self): pass\n    def send(self: State['Channel', Literal['ready']]): pass\ndef run():\n    channel = Channel()\n    channel.close()\n    channel.send()\n"
        ),
    );
    let changed = SourceFile::new(
        "main.py",
        "main",
        original
            .source
            .replace("to_state='closed'", "to_state='ready'"),
    );
    let mut session = codepact::AnalysisSession::default();
    for files in [[original.clone()], [changed], [original]] {
        let reused = session.check_with_ownership(&files, "3.13", Ownership::Advanced);
        let fresh = check_with_ownership(&files, "3.13", Ownership::Advanced);
        assert_eq!(reused, fresh);
    }
}
