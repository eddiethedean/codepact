Python API reference
====================

.. note::

   This reference is generated from the current source. ``move`` was added in
   0.5.0. The must-use helpers are available from 0.4.0.

The ``codepact`` package contains lightweight runtime helpers and static typing metadata. It does not load the analyzer executable when imported.

``codepact.resource``
---------------------

.. autofunction:: codepact.resource

The decorator accepts a literal method name in supported source. At runtime it attaches a ``Resource`` instance to the decorated class's ``__codepact_resource__`` attribute and returns the original class.

``closed_by`` must be a Python identifier. Invalid names raise ``ValueError``. Repeating the same lifecycle declaration is allowed; declaring a different lifecycle on a class that already has its own CodePact metadata raises ``ValueError``. The decorator does not check whether the named method exists or releases a resource.

The runtime helper also accepts ``resource(Connection, closed_by="close")``. Source inspection recognizes the supported class-decorator syntax; a runtime call does not imply that the analyzer resolves that call as a resource declaration.

``codepact.must_use`` and ``codepact.discard``
----------------------------------------------

.. autofunction:: codepact.must_use

.. autofunction:: codepact.discard

``must_use`` attaches ``MustUseResult`` metadata to and returns the original
callable. It preserves the callable's parameters, result type and identity; it
does not wrap or enforce the function at runtime. The source checker uses the
marker on supported synchronous returns. Conflicting existing metadata raises
``ValueError``.
The argument to ``discard`` is evaluated before the call. The helper returns
``None`` and leaves ownership and cleanup unchanged.

Typed outcomes
--------------

The outcome values are pure-Python frozen dataclasses. Their payloads retain
ordinary Python identity and behavior; the tags do not transfer ownership or
perform cleanup.

.. autoclass:: codepact.outcomes.Ok
   :members:

.. autoclass:: codepact.outcomes.Err
   :members:

.. autoclass:: codepact.outcomes.Some
   :members:

.. autoclass:: codepact.outcomes.Nothing

.. py:data:: codepact.Result

   Covariant annotation alias for ``Ok[T] | Err[E]``.

.. py:data:: codepact.Option

   Covariant annotation alias for ``Some[T] | Nothing``.

.. autofunction:: codepact.from_nullable

.. autofunction:: codepact.to_nullable

.. autofunction:: codepact.capture

.. autofunction:: codepact.capture_async

``capture(function, exceptions, /, *args, **kwargs)`` calls the supplied
callable once with the remaining arguments. It returns ``Ok(result)`` on
success or ``Err(exception)`` for a selected exception. An empty tuple,
a non-tuple, or an element other than an ``Exception`` subclass raises
``TypeError`` before the callable runs. Other exceptions propagate, including
cancellation, process-control exceptions, and exception groups.
``capture_async`` applies the same rules when awaited. It invokes and awaits
the callable in the current task.

``from_nullable(None)`` returns ``Nothing()``. Other values, including falsy
ones, become ``Some(value)``. ``to_nullable`` returns the payload from
``Some`` and returns ``None`` for ``Nothing``. After conversion,
``Some(None)`` and ``Nothing()`` both produce ``None``.

Persistent outcome acknowledgment
---------------------------------

.. py:data:: codepact.MustHandle

   Aliases ``Annotated[T, MustHandleOutcome()]``. Apply it to the outer
   ``Result`` or ``Option`` in a callable's return annotation. The checker
   tracks the delivered duty through supported assignments and aliases.

.. autoclass:: codepact.contracts.MustHandleOutcome

.. autofunction:: codepact.discard_outcome

Pass a ``Result`` or ``Option`` value and a keyword-only ``reason``.
An empty or whitespace-only reason, or a value of another type for
``reason``, raises ``ValueError``. A valid call returns ``None`` and
performs no cleanup. Use a literal reason to include the waiver in static
review. See :doc:`contracts` for a complete persistent-outcome example.

Finite state protocols
----------------------

.. autofunction:: codepact.state_machine

Pass ``states`` as a nonempty iterable of unique, non-keyword Python
identifiers. ``initial`` names one declared state. The helper returns a
decorator that attaches metadata and preserves class identity. Invalid
names, duplicates, an unknown initial state, and conflicting class
metadata raise ``ValueError``.

.. autofunction:: codepact.transition

``from_state`` takes one state name or a tuple of allowed state names.
``to_state`` names the normal completion state. ``on_raise`` and
``on_cancel`` optionally name states for an exceptional or cancelled exit.
The returned decorator attaches metadata and preserves the callable.
Invalid names, duplicate input states, and conflicting metadata raise
``ValueError``. The analyzer resolves state membership and interprets
omitted exceptional or cancellation effects as unknown in supported cases.

.. py:data:: codepact.State

   ``State[T, Literal["ready"]]`` is an annotation alias for
   ``Annotated[T, StateAccess[Literal["ready"]]]``. The supported
   receiver-only method form asserts a state precondition. It is also
   exported from ``codepact.contracts`` and ``codepact.rust``.

.. autoclass:: codepact.contracts.StateAccess

These helpers declare metadata. Use the :doc:`ownership` guide for the
advanced-contract opt-in, literal declaration forms, and a complete
precondition and transition example.

``codepact.move``
-----------------

.. autofunction:: codepact.move

The helper returns the same object with the same underlying type. With
experimental ownership enabled, the checker treats a supported call as an
explicit transfer of the tracked handle and its cleanup duty.
See :doc:`ownership` for transfer and TaskGroup examples.

Contract metadata
-----------------

.. autoclass:: codepact.contracts.Resource
   :members:

.. autoclass:: codepact.contracts.SharedBorrow
   :members:

.. autoclass:: codepact.contracts.OwnedAccess
   :members:

.. autoclass:: codepact.contracts.ExclusiveBorrow
   :members:

.. autoclass:: codepact.contracts.MustUseResult
   :members:

These metadata values are frozen dataclasses with slots. ``Resource`` stores a ``closed_by: str`` field; the access markers have no fields. Constructing ``Resource`` directly stores the supplied string without the decorator's identifier validation. The inspector validates recognized lifecycle declarations independently, using the selected source target's identifier rules.

Annotation aliases
-------------------

.. py:data:: codepact.contracts.Borrowed

   ``Borrowed[T]`` expands to ``Annotated[T, SharedBorrow()]`` and declares shared access.

.. py:data:: codepact.contracts.Owned

   ``Owned[T]`` expands to ``Annotated[T, OwnedAccess()]`` and declares owned access.

.. py:data:: codepact.contracts.BorrowedMut

   ``BorrowedMut[T]`` expands to ``Annotated[T, ExclusiveBorrow()]`` and declares exclusive access.

.. py:data:: codepact.contracts.MustUse

   ``MustUse[T]`` expands to ``Annotated[T, MustUseResult()]``. It requests immediate handling of supported synchronous function results; it is not an ownership or resource-cleanup contract.

The aliases are available from both ``codepact.contracts`` and ``codepact.rust``. See :doc:`contracts` for examples and semantics.

Python typing compatibility
---------------------------

The distribution includes ``py.typed``. Each alias preserves the underlying type for ordinary type checkers: a value annotated ``Borrowed[Connection]`` retains the ``Connection`` interface. CodePact metadata does not make incompatible Python types assignable or add runtime ownership checks.

Public imports and examples
---------------------------

.. py:data:: codepact.Resource

   Re-export of :py:class:`codepact.contracts.Resource`.

.. py:data:: codepact.MustUseResult

   Re-export of :py:class:`codepact.contracts.MustUseResult`.

Use ``codepact.rust`` or ``codepact.contracts`` for the access
aliases. Outcome types and adapters are available through ``codepact``,
``codepact.outcomes``, and ``codepact.rust``. The ``codepact.rust``
namespace supplies Python declarations and helpers.

.. code-block:: python

   from codepact import discard, move, must_use, resource
   from codepact.rust import Borrowed, MustUse

   @resource(closed_by="close")
   class Connection:
       def close(self) -> None:
           pass

   def inspect_connection(connection: Borrowed[Connection]) -> str:
       return type(connection).__name__

   @must_use
   def load_settings() -> int:
       return 1

   def load_port() -> MustUse[int]:
       return 8080

   connection = Connection()
   print(inspect_connection(connection))
   receiving_handle = move(connection)
   receiving_handle.close()
   settings = load_settings()
   discard(load_port())

The Python calls demonstrate the runtime interface. Use :doc:`quickstart`
for a checked lifecycle example and :doc:`ownership` for the conditions under
which the checker tracks a move.
