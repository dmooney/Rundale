#!/usr/bin/env python3
"""Entry point: python3 promptfoo/prompt-lab/lab.py --help"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from promptlab.__main__ import main  # noqa: E402

raise SystemExit(main())
