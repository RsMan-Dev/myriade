"""The wasm mod builders share an exact Rust release."""

import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class RustPinTests(unittest.TestCase):
    def test_the_pin_is_one_exact_release(self):
        self.assertRegex((ROOT / "tools/ci/rust-version").read_text(), r"^\d+\.\d+\.\d+\n?$")

    def test_the_wasm_toolchain_reads_the_pin(self):
        for path, include in [("crates/cargo-horizon-mod/src/wasm.rs", '"../../../tools/ci/rust-version"')]:
            with self.subTest(path=path):
                self.assertIn(f"include_str!({include}).trim_ascii()", (ROOT / path).read_text())


if __name__ == "__main__":
    unittest.main()
