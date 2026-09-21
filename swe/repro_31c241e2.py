import sphinx.ext.autodoc.mock as _m
print("mock module from:", _m.__file__)
from typing import TypeVar
from sphinx.ext.autodoc.mock import _MockModule, _MockObject

mock = _MockModule("mocked_module")
T = TypeVar("T")

class SubClass2(mock.SomeClass[T]):
    """docstring of SubClass"""

obj2 = SubClass2()
assert SubClass2.__doc__ == "docstring of SubClass"
assert isinstance(obj2, SubClass2)
print("REPRO-PASS: mock.SomeClass[TypeVar] works; subclass + instance OK")
