#!/usr/bin/env python3
"""
test_package.py — Unit tests for whitelisted package assembly and manifest (A2.4).
Pure Python standard library (unittest).
"""

from __future__ import annotations

import json
import os
import shutil
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path

# Add scripts dir to sys.path
sys.path.insert(0, str(Path(__file__).resolve().parent))
import package


class TestPackageAssembly(unittest.TestCase):
    def setUp(self):
        self.tmp_dir = Path(tempfile.mkdtemp(prefix="reforge_pkg_test_"))
        self.source_dir = self.tmp_dir / "source"
        self.source_dir.mkdir()

        # Create some allowed files
        (self.source_dir / "README.md").write_text("# Reforge Deploy")
        config = self.source_dir / "config"
        config.mkdir()
        (config / "auth.toml").write_text("port = 30001\n")
        (config / "channel.toml").write_text("port = 30003\n")
        schema = self.source_dir / "schema"
        schema.mkdir()
        (schema / "schema.sql").write_text("-- schema")
        (schema / "seed.sql").write_text("-- seed")

        # Create a dirty artifact that must be ignored/rejected
        logs = self.source_dir / "logs"
        logs.mkdir()
        (logs / "auth.log").write_text("secret log info")
        (self.source_dir / "dirty.dump").write_text("dirty dump")

    def tearDown(self):
        shutil.rmtree(self.tmp_dir, ignore_errors=True)

    def test_allowlist_isolation(self):
        files = package.collect_allowed_files(self.source_dir, include_binaries=False)
        rel_paths = [f[0] for f in files]
        self.assertIn("README.md", rel_paths)
        self.assertIn("config/auth.toml", rel_paths)
        self.assertIn("schema/schema.sql", rel_paths)
        self.assertNotIn("dirty.dump", rel_paths)
        self.assertNotIn("logs/auth.log", rel_paths)


    def test_package_assembly_and_verification_roundtrip(self):
        out_dir = self.tmp_dir / "pkg"
        pkg_dir, archive, manifest = package.assemble_package(
            source_dir=self.source_dir,
            out_dir=out_dir,
            include_binaries=False,
            create_archive=True,
            target="test-trg",
        )
        self.assertTrue(pkg_dir.exists())
        self.assertIsNotNone(archive)
        self.assertTrue(archive.exists())
        self.assertEqual(manifest["file_count"], 5)

        # Verify directory
        ok_dir, errs_dir = package.verify_package(pkg_dir)
        self.assertTrue(ok_dir, "Directory verification failed")
        self.assertEqual(errs_dir, [])

        # Verify zip archive
        ok_zip, errs_zip = package.verify_package(archive)
        self.assertTrue(ok_zip, "Zip verification failed")
        self.assertEqual(errs_zip, [])


    def test_determinism(self):
        pkg1, arch1, man1 = package.assemble_package(
            source_dir=self.source_dir,
            out_dir=self.tmp_dir / "pkg1",
            include_binaries=False,
            create_archive=False,
        )
        pkg2, arch2, man2 = package.assemble_package(
            source_dir=self.source_dir,
            out_dir=self.tmp_dir / "pkg2",
            include_binaries=False,
            create_archive=False,
        )
        self.assertEqual(man1, man2)


    def test_verify_detects_tampered_file(self):
        out_dir = self.tmp_dir / "pkg_tamper"
        pkg_dir, _, _ = package.assemble_package(
            source_dir=self.source_dir,
            out_dir=out_dir,
            include_binaries=False,
            create_archive=False,
        )
        # Tamper with a text file
        (pkg_dir / "config" / "auth.toml").write_text("port = 99999\n")

        ok, errors = package.verify_package(pkg_dir)
        self.assertFalse(ok)
        self.assertTrue(any("Checksum mismatch" in e for e in errors))


    def test_verify_detects_prohibited_file(self):
        out_dir = self.tmp_dir / "pkg_prohibited"
        pkg_dir, _, _ = package.assemble_package(
            source_dir=self.source_dir,
            out_dir=out_dir,
            include_binaries=False,
            create_archive=False,
        )
        # Insert a prohibited file
        (pkg_dir / "surreptitious.dump").write_text("private bits")

        ok, errors = package.verify_package(pkg_dir)
        self.assertFalse(ok)
        self.assertTrue(any("Prohibited file" in e for e in errors))


    def test_verify_detects_unlisted_file(self):
        out_dir = self.tmp_dir / "pkg_unlisted"
        pkg_dir, _, _ = package.assemble_package(
            source_dir=self.source_dir,
            out_dir=out_dir,
            include_binaries=False,
            create_archive=False,
        )
        # Insert a non-prohibited but unlisted file
        (pkg_dir / "extra.txt").write_text("hello")

        ok, errors = package.verify_package(pkg_dir)
        self.assertFalse(ok)
        self.assertTrue(any("Unlisted file" in e for e in errors))


if __name__ == "__main__":
    unittest.main()
