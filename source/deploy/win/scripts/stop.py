#!/usr/bin/env python3
import sys
from manage import main

if __name__ == "__main__":
    sys.exit(main(["stop"] + sys.argv[1:]))
