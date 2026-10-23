# Records

Tests of `TYPE … END TYPE`, which the Python compiler does not have (see
[docs/records.md](../../docs/records.md)). `cargo test -p zxr-cli --test
records` compiles `records_test.bas` at `-O0`, `-O2` and `-O4`, compares its
lowered text with `records_test.lowered.bas`, and checks the messages for
`records_errors.bas` against `records_errors.err`.

`records_test.bas` checks itself when run; with NextBuild's far-call emulator
(`Sources/Tests/BankedCode/run_far.py`, which needs the `z80` Python module):

```
zxbc --arch=zxnext -O2 -S 32768 records_test.bas -o /tmp/r.bin -M /tmp/r.map
python3 run_far.py /tmp/r.bin /tmp/r.banks.json /tmp/r.map 32768 records_test.bas
```

The lowered text is plain BASIC, so the Python compiler can build it too:

```
ZXR_DUMP_RECORDS=/tmp/low.bas zxbc --arch=zxnext -O2 -S 32768 records_test.bas -o /tmp/r.bin
python3 reference/zxbasic/zxbc.py --arch=zxnext -O2 -S 32768 /tmp/low.bas -o /tmp/p.bin
```

The two binaries differ only where this compiler fixes the Python backend:
PEEK(FIXED, …), PEEK(FLOAT, …) and POKE FLOAT at a constant address
(`tests/fixes/const_address.bas`), which the FIXED and FLOAT fields of a
global record use.
