#!/usr/local/bin/python3
# martysama0134's script to switch the compiler from the CC CXX variables
makeList=(
    './Extern/cryptopp/GNUmakefile',
    './Server/db/src/Makefile',
    './Server/game/src/Makefile',
    './Server/game/src/quest/Makefile',
    './Server/libgame/src/Makefile',
    './Server/liblua/5.0/config',
    './Server/libpoly/Makefile',
    './Server/libsql/Makefile',
    './Server/libthecore/src/Makefile',
    './Server/Makefile',
)

if __name__ == '__main__':
    for fn in makeList:
        try:
            lines = open(fn, "r").readlines()
        except FileNotFoundError:
            print("%s not found"%fn)
            continue
        # processing
        for n,ln in enumerate(lines):
            if ln.startswith("CC ") or ln.startswith("CC\t") or ln.startswith("CC="):
                lines[n] = "CC = clang\n"
            elif ln.startswith("CXX ") or ln.startswith("CXX\t") or ln.startswith("CXX="):
                lines[n] = "CXX = clang++\n"
        # saving
        with open(fn, "w") as f1:
            f1.write("".join(lines))
