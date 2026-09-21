import sys
from typing import TypeVar
from sphinx.ext.autodoc.mock import _MockModule, _MockObject

# prove we are testing the checkout tree, not an installed copy
_mockmod = sys.modules['sphinx.ext.autodoc.mock']
print("mock module from:", _mockmod.__file__)
assert 'sphinx-doc__sphinx-7889' in _mockmod.__file__, "WRONG TREE"

mock = _MockModule("mocked_module")
T = TypeVar("T")

class SubClass2(mock.SomeClass[T]):
    """docstring of SubClass"""

obj2 = SubClass2()
assert SubClass2.__doc__ == "docstring of SubClass"
assert isinstance(obj2, SubClass2)
print("REPRO-PASS: subscripting a mock with a TypeVar works; subclass + instance OK")
