#!/usr/bin/env python3
import sys
from manage import main

if __name__ == "__main__":
    sys.exit(main(["start"] + sys.argv[1:]))
