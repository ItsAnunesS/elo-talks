import unittest
from unittest.mock import patch

import live_test


class SessionPolicyTests(unittest.TestCase):
    def test_only_active_unlocked_session_is_safe(self):
        session = {"Active": True, "LockedHint": False, "State": "active"}
        manager = {"PreparingForSleep": False}
        cases = [
            (manager, session, True),
            ({"PreparingForSleep": True}, session, False),
            (manager, {**session, "LockedHint": True}, False),
            (manager, {**session, "Active": False}, False),
            (manager, {**session, "State": "closing"}, False),
            (manager, {**session, "State": "unknown"}, False),
        ]
        for manager_state, session_state, expected in cases:
            with self.subTest(manager=manager_state, session=session_state):
                with patch.object(
                    live_test, "properties", side_effect=[manager_state, session_state]
                ):
                    self.assertEqual(
                        live_test.safe_session(None, "/synthetic"), expected
                    )

    def test_missing_monitor_information_never_returns_safe(self):
        with patch.object(
            live_test, "properties", side_effect=RuntimeError("monitor unavailable")
        ):
            with self.assertRaises(RuntimeError):
                live_test.safe_session(None, "/synthetic")
        with patch.object(live_test, "properties", side_effect=[{}, {}]):
            with self.assertRaises(KeyError):
                live_test.safe_session(None, "/synthetic")


if __name__ == "__main__":
    unittest.main()
