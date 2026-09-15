#!/usr/bin/env python3
"""Compatibility wrapper: convert K.O. Racing models to Wavefront OBJ.

See ``kora/model.py`` for the format description; ``kora`` is the real
implementation.  Usage::

    model2obj.py <models-dir> <output-dir>
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from kora.cli import cmd_obj  # noqa: E402


class _Args:
    def __init__(self, models_dir, out_dir):
        self.models_dir = models_dir
        self.out_dir = out_dir


if __name__ == "__main__":
    sys.exit(cmd_obj(_Args(sys.argv[1] if len(sys.argv) > 1 else "assets/models",
                           sys.argv[2] if len(sys.argv) > 2 else "assets/obj")))
