import copy
import unittest

from scripts.rehearse_migration import rehearse


class RehearseMigrationTests(unittest.TestCase):
    def test_success_is_non_mutating_and_reports_count(self):
        snapshot = {"schemaVersion": 1, "state": {"admin": "GADMIN"}}
        original = copy.deepcopy(snapshot)
        plan = {
            "targetVersion": 3,
            "steps": [
                {"from": 1, "to": 2, "requiredKeys": ["admin"], "set": {"paused": False}},
                {"from": 2, "to": 3, "requiredKeys": ["paused"], "set": {"feeBps": 100}},
            ],
            "finalRequiredKeys": ["admin", "paused", "feeBps"],
        }

        result = rehearse(snapshot, plan)

        self.assertTrue(result["success"])
        self.assertEqual(result["migrationCount"], 2)
        self.assertEqual(snapshot, original)

    def test_missing_step_fails(self):
        result = rehearse(
            {"schemaVersion": 1, "state": {}},
            {"targetVersion": 3, "steps": [{"from": 2, "to": 3}]},
        )
        self.assertFalse(result["success"])
        self.assertIn("missing migration", result["failures"][0])

    def test_missing_required_key_fails(self):
        result = rehearse(
            {"schemaVersion": 1, "state": {}},
            {
                "targetVersion": 2,
                "steps": [{"from": 1, "to": 2, "requiredKeys": ["admin"]}],
            },
        )
        self.assertFalse(result["success"])
        self.assertIn("required key", result["failures"][0])


if __name__ == "__main__":
    unittest.main()
