# PRIVATE

Tests of `PRIVATE SUB` / `PRIVATE FUNCTION`, which the Python compiler does
not have. `cargo test -p zxr-cli --test private` checks the diagnostics and
the shape of the generated code. `private_test.bas` checks itself when run;
with NextBuild's far-call emulator (`Sources/Tests/BankedCode/run_far.py`,
which needs the `z80` Python module; use NextBuild's own copy, not the
snapshot in `reference/corpus`, which predates fixes to the emulator and
fails `tests/fixes/emulator_check.bas`):

```
zxbc --arch=zxnext -O4 -S 32768 --code-bank-base 40 private_test.bas -o /tmp/p.bin -M /tmp/p.map
python3 run_far.py /tmp/p.bin /tmp/p.banks.json /tmp/p.map 32768 private_test.bas
```
