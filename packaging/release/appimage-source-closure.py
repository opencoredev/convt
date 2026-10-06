#!/usr/bin/env python3
"""Assemble the AppImage dependency source/recipe/notice bundle."""

import importlib.util
import pathlib
import sys

sys.dont_write_bytecode = True
path = pathlib.Path(__file__).with_name("native-source-tools.py")
spec = importlib.util.spec_from_file_location("native_source_tools", path)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

if __name__ == "__main__":
    module.main(path.with_name("appimage-source-closure.lock.json"))
