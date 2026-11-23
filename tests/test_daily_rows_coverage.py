"""Every test moved to the daily run is run by some leg of the daily matrix (tools/ci/check_ignored_test_rows.py)."""
import importlib.util
import json
import subprocess
import sys
import unittest
import unittest.mock
from pathlib import Path

from tools.ci import ignored_shards, ignored_tests

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("check_ignored_test_rows", ROOT / "tools/ci/check_ignored_test_rows.py")
check = importlib.util.module_from_spec(spec)
spec.loader.exec_module(check)

FULL = [*ignored_shards.expand(ignored_shards.SHARDS), *ignored_shards.DAILY_ROWS_LEGS.values()]


def row(package, test, target="lib"):
    return ignored_tests.Row(package, test, target, "daily", f"daily-{package}.txt", "slow")


class DailyRowsCoverage(unittest.TestCase):
    TARGETS = {"horizon-world": {"lib", "test:integration"}, "horizon-server": {"lib", "test:tunnel"},
               "myriade-engine": {"lib", "bin:myriade-engine", "test:lan_bot"}}

    def test_the_committed_rows_all_run_in_the_full_daily_matrix(self):
        rows = ignored_tests.daily_rows()
        self.assertTrue(rows)
        self.assertEqual(check.daily_problems(rows, ignored_shards.workspace_targets(), FULL), [])

    def test_the_full_matrix_has_the_daily_rows_leg(self):
        printed = subprocess.run([sys.executable, str(ROOT / "tools/ci/ignored_shards.py"), "--matrix", "--all"],
                                 capture_output=True, text=True, check=True).stdout
        legs = json.loads(printed.removeprefix("matrix="))["shard"]
        self.assertIn(ignored_shards.DAILY_ROWS, legs)

    def test_a_row_no_leg_runs_fails(self):
        rows = [row("horizon-world", "a"), row("horizon-server", "b", "test:tunnel")]
        self.assertEqual(check.daily_problems(rows, self.TARGETS, FULL), [])
        # Without the daily-rows leg the relay's integration test would run nowhere; the world's library is its shard's.
        problems = check.daily_problems(rows, self.TARGETS, ignored_shards.expand(ignored_shards.SHARDS))
        self.assertEqual(len(problems), 1)
        self.assertIn("horizon-server b: no leg runs it", problems[0])
        problems = check.daily_problems(rows, self.TARGETS, [ignored_shards.DAILY_ROWS])
        self.assertEqual(len(problems), 1)
        self.assertIn("no leg runs horizon-world's shard", problems[0])

    def test_a_row_must_name_a_package_and_one_of_its_targets(self):
        problems = check.daily_problems([row("horizon-nothing", "a"), row("horizon-server", "b", "test:missing")],
                                        self.TARGETS, FULL)
        self.assertIn("horizon-nothing is not a workspace package", problems[0])
        self.assertIn("horizon-server has no target test:missing", problems[1])

    def test_each_row_runs_in_exactly_one_place(self):
        rows = [row("horizon-world", "a"), row("horizon-world", "b", "test:integration"),
                row("myriade-engine", "c", "test:lan_bot"), row("horizon-server", "d")]
        groups = ignored_tests.daily_rows_groups(rows)
        for entry in rows:
            in_shard = ignored_tests.shard_runs(entry.package, entry.reference)
            in_leg = entry.test in groups.get((entry.package, entry.reference), ())
            self.assertNotEqual(in_shard, in_leg, entry)

    def test_the_command_passes_on_the_committed_rows(self):
        result = subprocess.run([sys.executable, str(ROOT / "tools/ci/check_ignored_test_rows.py")],
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertIn("; 0 problems", result.stdout)


class DailyRowsMatchTheSource(unittest.TestCase):
    """The rows and the `#[ignore = "daily"]` attributes agree both ways, by package and function name."""
    CLIENT = {"myriade-engine", "myriade-engine-lib"}

    def test_the_committed_rows_and_attributes_agree(self):
        functions = check.daily_functions()
        self.assertTrue(functions)
        self.assertEqual(check.source_problems(ignored_tests.daily_rows(), functions), [])

    def test_an_attribute_without_a_row_fails_with_what_to_do(self):
        problems = check.source_problems([], [("crates/horizon-server/tests/tunnel.rs", 9, "slow", {"horizon-server"})])
        self.assertEqual(len(problems), 1)
        self.assertTrue(problems[0].startswith("crates/horizon-server/tests/tunnel.rs:9: `slow` is #[ignore"))
        self.assertIn("tools/ci/move_to_daily.py apply", problems[0])

    def test_a_row_without_an_attribute_fails_with_what_to_do(self):
        problems = check.source_problems([row("horizon-server", "gone", "test:tunnel")], [])
        self.assertEqual(len(problems), 1)
        self.assertIn("horizon-server gone: no function `gone`", problems[0])
        self.assertIn("restore the attribute", problems[0])

    def test_a_function_built_into_two_targets_has_one_attribute_and_a_row_per_target(self):
        functions = [("crates/myriade-level/src/gfx/sunrise_fan.rs", 186, "angular", {"myriade-level"})]
        rows = [row("myriade-level", "gfx::sunrise_fan::tests::angular"),
                row("myriade-level", "sunrise_fan::tests::angular", "test:sunrise_shader")]
        self.assertEqual(check.source_problems(rows, functions), [])

    def test_a_shared_source_folder_belongs_to_both_packages(self):
        owners = check.source_packages()
        self.assertEqual(owners["crates/myriade-engine"], self.CLIENT)
        functions = [("crates/myriade-engine/src/save.rs", 3, "slow", self.CLIENT)]
        self.assertEqual(check.source_problems([row("myriade-engine-lib", "save::tests::slow")], functions), [])
        self.assertEqual(len(check.source_problems([row("horizon-world", "save::tests::slow")], functions)), 2)



class RowlessIgnores(unittest.TestCase):
    """An #[ignore]d test no daily shard runs needs a row of some mark; in a shard's own targets it runs anyway."""

    def ignored(self, path, packages, kind, name="slow"):
        return (path, 7, name, packages, kind, False)

    def test_only_tests_no_shard_runs_need_a_row(self):
        functions = [self.ignored("crates/horizon-world/src/a.rs", {"horizon-world"}, "lib"),
                     self.ignored("crates/myriade-engine/src/game/b.rs", {"myriade-engine"}, "lib"),
                     self.ignored("crates/horizon-world/tests/c.rs", {"horizon-world"}, "test"),
                     self.ignored("crates/horizon-core/src/d.rs", {"horizon-core"}, "lib")]
        problems = check.rowless_problems([], functions)
        self.assertEqual([problem.split(":", 1)[0] for problem in problems],
                         ["crates/horizon-world/tests/c.rs", "crates/horizon-core/src/d.rs"])
        self.assertIn("runs nowhere", problems[0])
        self.assertIn("manual-PACKAGE.txt", problems[0])
        rows = [ignored_tests.Row("horizon-world", "it::slow", "by-hand", "manual", "manual-horizon-world.txt"),
                ignored_tests.Row("horizon-core", "d::slow", "#12", None, "issue-12.txt")]
        self.assertEqual(check.rowless_problems(rows, functions), [])

    def test_integration_package_examples_and_benches_need_a_row(self):
        uncovered = [self.ignored("crates/myriade-engine/examples/a.rs", {"myriade-engine"}, "example"),
                     self.ignored("crates/myriade-engine/benches/b.rs", {"myriade-engine"}, "bench")]
        problems = check.rowless_problems([], uncovered)
        self.assertEqual([problem.split(":", 1)[0] for problem in problems],
                         ["crates/myriade-engine/examples/a.rs", "crates/myriade-engine/benches/b.rs"])

        covered = [self.ignored("crates/myriade-engine/src/lib.rs", {"myriade-engine"}, "lib"),
                   self.ignored("crates/myriade-engine/src/bin/a.rs", {"myriade-engine"}, "bin"),
                   self.ignored("crates/myriade-engine/tests/b.rs", {"myriade-engine"}, "test")]
        self.assertEqual(check.rowless_problems([], covered), [])

    def test_where_a_source_file_builds(self):
        self.assertEqual([check.target_kind("crates/x", path) for path in (
            "crates/x/src/lib.rs", "crates/x/src/main.rs", "crates/x/src/bin/tool.rs", "crates/x/tests/it.rs",
            "crates/x/examples/probe.rs", "crates/x/src/a/tests.rs")], ["lib", "bin", "bin", "test", "example", "lib"])



class DailyTargetSelection(unittest.TestCase):
    def test_daily_selection_is_exact_sorted_and_excludes_manual_rows(self):
        rows = [row("horizon-mod-package", "b::slow"), row("horizon-mod-package", "a::slow"),
                row("horizon-mod-package", "it::slow", "test:it"), row("horizon-world", "c::slow")]
        with unittest.mock.patch.object(ignored_tests, "daily_rows", return_value=rows):
            self.assertEqual(ignored_tests.daily_tests("horizon-mod-package", "lib"), ["a::slow", "b::slow"])
        printed = subprocess.run([sys.executable, str(ROOT / "tools/ci/ignored_tests.py"), "--daily-tests",
                                  "horizon-mod-package", "lib"], capture_output=True, text=True, check=True).stdout
        self.assertIn("holder::tests::private_version_is_refused_before_normal_startup", printed.split())
        self.assertNotIn("holder::game_entry::tests::write_missing_registration_actor", printed.split())
        # No rows: no output at all, not one empty name.
        empty = subprocess.run([sys.executable, str(ROOT / "tools/ci/ignored_tests.py"), "--daily-tests",
                                "no-such-package", "lib"], capture_output=True, text=True, check=True).stdout
        self.assertEqual(empty, "")


if __name__ == "__main__":
    unittest.main()
