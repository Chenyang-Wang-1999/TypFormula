"""Headless checks for release paths and packaging input validation."""
import os
import struct
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
from desktop import runtime
from tools import build_release


class ReleaseTest(unittest.TestCase):
    def test_frozen_helpers_never_fall_back_to_checkout(self):
        with tempfile.TemporaryDirectory(prefix='发布 空格-') as directory:
            root = Path(directory)
            with patch.object(sys, 'frozen', True, create=True), patch.object(sys, '_MEIPASS', str(root), create=True), patch.dict(os.environ, {}, clear=True):
                self.assertEqual(runtime.resource_root(), root)
                self.assertEqual(runtime.helper('typformula.exe', 'TYPFORMULA_BIN', 'server'), root / 'bin/typformula.exe')
                self.assertIsNone(runtime.bundled_tinymist())
                (root / 'bin').mkdir(); (root / 'bin/tinymist.exe').touch()
                self.assertEqual(runtime.bundled_tinymist(), root / 'bin/tinymist.exe')
                os.environ['TYPFORMULA_BIN'] = str(root / 'custom.exe')
                self.assertEqual(runtime.helper('typformula.exe', 'TYPFORMULA_BIN', 'server'), root / 'custom.exe')

    def test_frozen_workspace_is_user_writable_location(self):
        with patch.object(sys, 'frozen', True, create=True), patch.dict(os.environ, {'LOCALAPPDATA': r'C:\Users\example\AppData\Local'}):
            self.assertEqual(runtime.default_workspace(), Path(os.environ['LOCALAPPDATA']) / 'TypFormula/workspace')
        with patch.object(sys, 'frozen', False, create=True):
            self.assertEqual(runtime.default_workspace(), runtime.resource_root() / 'workspace')

    def test_missing_explicit_tinymist_does_not_pick_another_version(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(ValueError):
                build_release.find_tinymist(Path(directory) / 'missing.exe')

    def test_x64_helpers_are_required(self):
        with tempfile.TemporaryDirectory() as directory:
            file = Path(directory) / 'helper.exe'
            image = bytearray(256); image[:2] = b'MZ'
            struct.pack_into('<I', image, 0x3c, 128)
            image[128:134] = b'PE\0\0\x64\x86'; file.write_bytes(image)
            build_release.require_x64(file)
            image[132:134] = b'\x64\xaa'; file.write_bytes(image)
            with self.assertRaises(ValueError):build_release.require_x64(file)

    def test_generated_spec_handles_unicode_spaces_and_quotes(self):
        with tempfile.TemporaryDirectory(prefix='发布 空格-') as directory:
            spec = Path(directory) / 'app.spec'
            build_release.write_spec(spec, [("C:/a'b/字体/font.otf", 'fonts')], [("C:/tools/helper.exe", 'bin')])
            compile(spec.read_text(encoding='utf-8'), str(spec), 'exec')


if __name__ == '__main__':
    unittest.main()
