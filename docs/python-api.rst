Python API reference
====================

The ``typepact`` package contains lightweight runtime helpers and static typing metadata. It does not load the analyzer executable when imported.

``typepact.resource``
---------------------

.. autofunction:: typepact.resource

The decorator accepts a literal method name in supported source. At runtime it attaches a ``Resource`` instance to the decorated class's ``__typepact_resource__`` attribute and returns the original class.

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

Annotation aliases
-------------------

.. autodata:: typepact.contracts.Borrowed

.. autodata:: typepact.contracts.Owned

.. autodata:: typepact.contracts.BorrowedMut

The aliases are available from both ``typepact.contracts`` and ``typepact.rust``. See :doc:`contracts` for examples and semantics.
