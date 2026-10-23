#!/usr/bin/env python3
"""Runs a self-checking program (see swap32_irq.bas) in NextBuild's far-call
emulator with an interrupt at every instruction of one runtime routine.

    irq_check.py RUN_FAR_DIR PROGRAM.bin PROGRAM.map [LABEL]

RUN_FAR_DIR is where `run_far.py` is (NextBuild's
`Sources/Tests/BankedCode`). LABEL defaults to `core.__SWAP32`, a routine of one-byte instructions
that ends at its first `ret`. Each time the
routine runs, an interrupt is taken before each of its instructions: the
return address is pushed and a handler at $0038 pushes and pops a word of
its own before it returns. Exits 1 if any check fails.
"""

import sys

ORG = 0x8000
RESULTS = 61440
HANDLER = 0x0038
# push hl / ld hl, $A55A / push hl / pop hl / pop hl / ret
HANDLER_CODE = bytes([0xE5, 0x21, 0x5A, 0xA5, 0xE5, 0xE1, 0xE1, 0xC9])


def main(run_far_dir, binpath, mappath, label="core.__SWAP32"):
    sys.path.insert(0, run_far_dir)
    import run_far

    labels = {}
    for line in open(mappath):
        if ": ." in line:
            addr, name = line.strip().split(": .", 1)
            if ":" not in addr:
                labels[name] = int(addr, 16)
    start = labels[label]
    pages = {page: bytearray(0x2000) for page in (run_far.BOOT_MMU[3],)}
    m = run_far.Next(open(binpath, "rb").read(), pages, org=ORG)
    # Up to its first `ret`: every instruction of the routine is one byte.
    end = start + m.read(start, 64).index(0xC9) + 1
    m.set_memory_block(HANDLER, HANDLER_CODE)
    sentinel = 0xFFF0
    m.sp = 0xFF00
    m.set_memory_block(m.sp, sentinel.to_bytes(2, "little"))
    m.pc = labels["core.__START_PROGRAM"]

    inside = set(range(start, end))
    stops = inside | {sentinel}
    taken = 0
    resumed = None  # the instruction an interrupt has just returned to
    while True:
        m.execute(stops)
        pc = m.pc
        if pc == sentinel:
            break
        if pc == resumed:
            resumed = None
            continue
        m.sp = (m.sp - 2) & 0xFFFF
        m.set_memory_block(m.sp, pc.to_bytes(2, "little"))
        m.pc = HANDLER
        resumed = pc
        taken += 1

    run = int.from_bytes(m.read(labels["_t_run"], 2), "little")
    failed = int.from_bytes(m.read(labels["_t_fail"], 2), "little")
    verdicts = m.read(RESULTS, run)
    for n, ok in enumerate(verdicts):
        if not ok:
            print(f"  [FAIL] check #{n + 1}")
    print(f"{taken} interrupts in {label}; {run} checks ran, {failed} failed")
    return 1 if failed or run == 0 else 0


if __name__ == "__main__":
    sys.exit(main(*sys.argv[1:]))
