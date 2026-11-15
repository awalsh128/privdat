import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent / "generated"))

import privdat  # noqa: E402


def test_version():
    assert privdat.version() == "0.1.0"


def test_qualified_name():
    col = privdat.ColumnRef(system="pg", table="users", column="email")
    assert privdat.qualified_name(col) == "pg.users.email"
