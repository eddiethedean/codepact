Python API reference
====================

The ``typepact`` package contains lightweight runtime helpers and static typing metadata. It does not load the analyzer executable when imported.

``typepact.resource``
---------------------

.. autofunction:: typepact.resource

The decorator accepts a literal method name in supported source. At runtime it attaches a ``Resource`` instance to the decorated class's ``__typepact_resource__`` attribute and returns the original class.

``closed_by`` must be a Python identifier. Invalid names raise ``ValueError``. Repeating the same lifecycle declaration is allowed; declaring a different lifecycle on a class that already has its own TypePact metadata raises ``ValueError``. The decorator does not check whether the named method exists or releases a resource.

The runtime helper also accepts ``resource(Connection, closed_by="close")``. Source inspection recognizes the supported class-decorator syntax; a runtime call does not imply that the analyzer resolves that call as a resource declaration.

Contract metadata
-----------------

.. autoclass:: typepact.contracts.Resource
   :members:

.. autoclass:: typepact.contracts.SharedBorrow
   :members:

.. autoclass:: typepact.contracts.OwnedAccess
   :members:

.. autoclass:: typepact.contracts.ExclusiveBorrow
   :members:

These metadata values are frozen dataclasses with slots. ``Resource`` stores a ``closed_by: str`` field; the access markers have no fields. Constructing ``Resource`` directly stores the supplied string without the decorator's identifier validation. The inspector validates recognized lifecycle declarations independently, using the selected source target's identifier rules.

Annotation aliases
-------------------

.. py:data:: typepact.contracts.Borrowed

   ``Borrowed[T]`` expands to ``Annotated[T, SharedBorrow()]`` and declares shared access.

.. py:data:: typepact.contracts.Owned

   ``Owned[T]`` expands to ``Annotated[T, OwnedAccess()]`` and declares owned access.

.. py:data:: typepact.contracts.BorrowedMut

   ``BorrowedMut[T]`` expands to ``Annotated[T, ExclusiveBorrow()]`` and declares exclusive access.

The aliases are available from both ``typepact.contracts`` and ``typepact.rust``. See :doc:`contracts` for examples and semantics.

Python typing compatibility
---------------------------

The distribution includes ``py.typed``. Each alias preserves the underlying type for ordinary type checkers: a value annotated ``Borrowed[Connection]`` retains the ``Connection`` interface. TypePact metadata does not make incompatible Python types assignable or add runtime ownership checks.
