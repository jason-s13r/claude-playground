import unittest

from __IDENT__ import greet


class TestGreet(unittest.TestCase):
    def test_greet(self) -> None:
        self.assertEqual(greet("world"), "hello from __NAME__, world")


if __name__ == "__main__":
    unittest.main()
