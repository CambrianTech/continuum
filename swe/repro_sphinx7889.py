"""Repro for sphinx issue #7886 (fixed upstream by PR #7889):
"autodoc: TypeError is raised on mocking generic-typed classes".

Run with the 7889 checkout first on sys.path:
    PYTHONPATH="<checkout>;<_env39>" <py3.9> swe/repro_sphinx7889.py

Mirrors the parametrized-type block that test_MockObject gains for this fix,
plus a plain-subclassing sanity check that already worked before.
"""
from typing import TypeVar

from sphinx.ext.autodoc.mock import _MockObject

mock = _MockObject()

# plain subclassing — worked before the bug was reported
class SubClass(mock.SomeClass):
    """docstring of SubClass"""


SubClass()

# parametrized type — #7886: subscripting a mock instance passes a TypeVar
# into _MockObject.__getitem__ -> _make_subclass, which then does
# str + TypeVar and raises TypeError.
T = TypeVar('T')


class SubClass2(mock.SomeClass[T]):
    """docstring of SubClass"""


obj2 = SubClass2()
assert SubClass2.__doc__ == "docstring of SubClass"
assert isinstance(obj2, SubClass2)

print("REPRO-OK: generic-typed mock subclass works")
