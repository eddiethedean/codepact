Python API reference
====================

.. note::

   This reference is generated from the 0.5.0 source. ``move`` was added in
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
marker on supported synchronous returns.
``discard`` evaluates the given value and returns ``None`` without changing
resource ownership, cleanup or exception behavior.

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

``codepact.rust`` also exports ``Borrowed``, ``Owned``, ``BorrowedMut``, and
``MustUse``. Use those aliases for short annotations or import the equivalent
``codepact.contracts`` aliases beside their metadata classes.

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
