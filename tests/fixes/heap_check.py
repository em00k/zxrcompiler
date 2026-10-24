#!/usr/bin/env python3
"""Runs a self-checking program (see string_heap.bas) in NextBuild's far-call
emulator, then walks the heap it leaves behind.

    heap_check.py RUN_FAR_DIR PROGRAM.bin PROGRAM.map HEAP_SIZE [VAR ...]

RUN_FAR_DIR is where NextBuild's `run_far.py` is
(`Sources/Tests/BankedCode`). HEAP_SIZE is the -H the program was built
with. Each VAR is a string variable (`k`, for k$): it must be empty or point
at a block of its own inside the heap, not at program memory, a free block or
another variable's block. The free list must stay inside the heap, in order,
without overlaps. Exits 1 if a check in the program or of the heap fails.
"""

import sys

ORG = 0x8000
RESULTS = 61440


def main(run_far_dir, binpath, mappath, heap_size, *names):
    sys.path.insert(0, run_far_dir)
    import run_far

    labels = {}
    for line in open(mappath):
        if ": ." in line:
            addr, name = line.strip().split(": .", 1)
            if ":" not in addr:
                labels[name] = int(addr, 16)
    pages = {page: bytearray(0x2000) for page in (run_far.BOOT_MMU[3],)}
    m = run_far.Next(open(binpath, "rb").read(), pages, org=ORG)
    sentinel = 0xFFF0
    m.sp = 0xFF00
    m.set_memory_block(m.sp, sentinel.to_bytes(2, "little"))
    m.pc = labels["core.__START_PROGRAM"]
    try:
        m.execute({sentinel}, max_steps=400_000)
    except RuntimeError as e:
        print(f"the program did not finish: {e}")
        return 1
    word = lambda a: int.from_bytes(m.read(a, 2), "little")

    bad = []
    run, failed = word(labels["_t_run"]), word(labels["_t_fail"])
    for n, ok in enumerate(m.read(RESULTS, run)):
        if not ok:
            bad.append(f"check #{n + 1} in the program")

    heap = labels["core.ZXBASIC_MEM_HEAP"]
    end = heap + int(heap_size)
    # The free list: a header block of size 0, then blocks of (size, next).
    free, at, seen = [], word(heap + 2), set()
    while at:
        if at in seen or not heap + 4 <= at < end:
            bad.append(f"free list reaches ${at:04X}, outside the heap ${heap:04X}-${end:04X}")
            break
        seen.add(at)
        size = word(at)
        if free and at < free[-1][0] + free[-1][1]:
            bad.append(f"free block ${at:04X} overlaps or precedes the one before it")
        if at + size > end:
            bad.append(f"free block ${at:04X} of {size} runs past the heap")
        free.append((at, size))
        at = word(at + 2)
    # Each string variable: empty, or a block of its own in the heap.
    owners = {}
    for name in names:
        ptr = word(labels[f"_{name}"])
        if ptr == 0:
            continue
        block = ptr - 2
        if not heap + 4 <= block < end:
            bad.append(f"{name}$ points at ${ptr:04X}, outside the heap")
        elif any(a <= block < a + s for a, s in free):
            bad.append(f"{name}$ points into a free block at ${ptr:04X}")
        if ptr in owners:
            bad.append(f"{name}$ and {owners[ptr]}$ share the block at ${ptr:04X}")
        owners[ptr] = name
    total = sum(s for _, s in free)
    print(f"{run} checks ran, {failed} failed; {len(free)} free blocks, {total} bytes free")
    for b in bad:
        print(f"  [FAIL] {b}")
    return 1 if bad or run == 0 else 0


if __name__ == "__main__":
    sys.exit(main(*sys.argv[1:]))
