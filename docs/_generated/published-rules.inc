<!-- Generated from CodePact 0.10.0 by sync_docs_reference.py. -->

| Rule | Introduced | Selection | Meaning |
| --- | --- | --- | --- |
| `TP1001` | 0.2.0 | Default | A modeled resource use occurs after its close transition. |
| `TP1002` | 0.3.0 | Default | An owned resource remains open on a modeled function exit. |
| `TP1003` | 0.3.0 | Default | An Owned[T] result violates the modeled handoff contract. |
| `TP2001` | 0.2.0 | Default | A borrowed resource is closed through a non-owning view. |
| `TP2002` | 0.5.0 | Opt-in | An operation uses a handle after it transferred ownership. |
| `TP2003` | 0.5.0 | Opt-in | The transfer lacks an eligible owner or conflicts with a live alias or borrow. |
| `TP2004` | 0.5.0 | Opt-in | A checked borrow is consumed or leaves its lexical region. |
| `TP2005` | 0.5.0 | Opt-in | A resolved Owned parameter received a tracked resource without an explicit transfer. |
| `TP2006` | 0.5.0 | Opt-in | Cleanup occurs while a tracked coroutine or task borrow is outstanding. |
| `TP2101` | 0.6.0 | Opt-in | A tracked resource is accessed through a competing alias while an exclusive borrow is active. |
| `TP2102` | 0.6.0 | Opt-in | A modeled operation may violate its finite state precondition, or a TaskGroup operation may occur outside the active group scope. |
| `TP3001` | 0.4.0 | Default | A declared must-use result is discarded as an expression statement. |
| `TP3003` | 0.9.0 | Opt-in | A locally submitted task, future, or pool job leaves scope without delivery of its value or worker exception. |
| `TP4001` | 0.1.0 | Mandatory | The source could not be parsed for its selected Python target. |
| `TP4002` | 0.1.0 | Mandatory | A declaration or inline policy directive is invalid. |
| `TP4101` | 0.9.0 | Opt-in | A selected synchronous signature has a known resource parameter with unspecified access. |
| `TP4102` | 0.9.0 | Opt-in | A selected synchronous API requires an owned resource handoff. |
| `TP4103` | 0.9.0 | Opt-in | A selected synchronous API requires an immediate-use declaration. |
| `TP5001` | 0.7.0 | Opt-in | A supported finite outcome match leaves a known variant uncovered. |
| `TP5002` | 0.9.0 | Default | The final local carrier of a persistent outcome is lost while acknowledgment is pending. |
| `TP5003` | 0.9.0 | Default | A MustHandle declaration or discard_outcome waiver is outside the supported form. |
| `TP5004` | 0.9.0 | Default | A newly acquired pending outcome leaves a callable without an outward MustHandle agreement. |
| `TP5005` | 0.9.0 | Default | A resolved persistent outcome was discarded with a recorded reason. |
| `TP6001` | 0.7.0 | Informational hint | A local class with a supported close method is passed to contextlib.closing. |
| `TP6002` | 0.7.0 | Informational hint | An unmarked synchronous wrapper directly returns a resolved must-use result. |
| `TP6003` | 0.9.0 | Informational hint | A wildcard acknowledged the sole remaining variant of a pending outcome. |
| `TP7001` | 0.10.0 | Opt-in | A queue operation violates the local accounting or shutdown protocol. |
| `TP7002` | 0.10.0 | Opt-in | A guard, permit or signal protocol is violated. |
| `TP7003` | 0.10.0 | Opt-in | A queue or connection protocol is violated. |
| `TP7004` | 0.10.0 | Opt-in | An acquisition, release or shared update violates the selected process protocol. |
| `TP7005` | 0.10.0 | Opt-in | A direct-script launch violates the reviewed spawn entry protocol. |
| `TP9001` | 0.1.0 | Mandatory | A construct or effect is outside the qualified analysis subset. |
| `TP9002` | 0.1.0 | Mandatory | The requested analysis did not complete. |
