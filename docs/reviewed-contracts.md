# Reviewed contracts and runtime models

Choose the guide for the evidence you have. Provider checks read exact wheel
contents and installed metadata. Runtime profiles select reviewed effects
for the Python program's deployment runtime.

## Publish and verify a provider

Package authors can export, build, and verify a contract manifest. Start with
the [complete provider project](publish-provider.md).

## Supply a project model

Application teams can select installed providers or declare reviewed effects
for one version of a dependency. See [installed provider checks and project models](consume-providers.md).

## Select the runtime profile

See [runtime configuration and runnable examples](runtime-models.md) for the
published CLI's file, asyncio, process, and work-result models.

### Work result observation

Run the [work and outcome example](runtime-models.md#try-result-observation-and-outcome-handling)
to distinguish a completed task, an observed result, and an acknowledged outcome.

## Queues, IPC and guards

These checks require CodePact 0.10. The [candidate concurrency guide](concurrency.md)
includes the build command, exact runtime and launch settings, and cases for
queue accounting, a guarded method, and direct-script spawn.

## Reviewed HTTP and SQL profiles

See [the installed artifact requirements and supported operations](consume-providers.md#reviewed-http-and-sql-profiles).
