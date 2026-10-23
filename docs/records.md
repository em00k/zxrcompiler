# Records: TYPE … END TYPE

A TYPE gives a block of memory named fields. Variables and arrays can be of
that type, and a pointer to one reaches its fields with `->`. The syntax is
FreeBASIC's. The Python compiler does not have it, so a program that uses a
TYPE builds with this compiler only. A program without one builds exactly
as before.

```basic
TYPE Bridge
    x, y   AS INTEGER
    w, h   AS UBYTE
    travel AS UBYTE
END TYPE

DIM br(1) AS Bridge                 ' two bridges, 7 bytes each

br(0).x = 120
IF abs(px - br(i).x) <= br(i).w / 2 THEN ...

FUNCTION OnBridge(b AS Bridge PTR, deck AS INTEGER) AS UBYTE
    RETURN abs(x - b->x) <= b->w / 2 AND abs(y - (b->y + deck)) <= b->h / 2
END FUNCTION

IF OnBridge(@br(0), deck0) THEN ...
```

For a first introduction, with four example programs each written the old
way and with records, see `Sources/Tests/Records/` in NextBuild
(`records.md` there; `run_records.py` checks the two versions agree).

## Declaring a TYPE

```basic
TYPE Name
    field AS type
    a, b AS type                    ' several of one type
    cells(7) AS UBYTE               ' an array: 0 TO 7
    grid(1 TO 4, 2) AS UINTEGER     ' bounds are numbers
    pos AS Point                    ' another TYPE, inside this one
    next AS Name PTR                ' a pointer, 2 bytes
END TYPE
```

- `TYPE` and `END TYPE` each start a line.
- A field is `BYTE`, `UBYTE`, `INTEGER`, `UINTEGER`, `LONG`, `ULONG`, `FIXED`,
  `FLOAT`, another TYPE, or `T PTR` for a TYPE `T`.
- Fields are laid out in order with no padding.
- An array field's bounds are numbers, and its lower bound is 0 unless given
  (`#pragma array_base` does not apply to it).
- `SIZEOF(Name)` is the size in bytes, a constant.
- A TYPE can be used before its definition; one cannot contain itself, except
  through a PTR.

## Using one

| Write | Meaning |
|---|---|
| `DIM v AS T` | one record (global, or local in a SUB) |
| `DIM a(7) AS T`, `DIM a(1 TO 8, 3) AS T` | an array of records, with the bounds of any array |
| `DIM v AS T AT 49152` | a record laid over fixed memory |
| `v.x`, `a(i).x`, `v.pos.y`, `v.cells(j)` | a field: read it, or assign it as a statement |
| `@v`, `@a(i)`, `@v.x` | an address, a UINTEGER |
| `DIM p AS T PTR`, a parameter `p AS T PTR`, `FUNCTION f() AS T PTR` | a pointer: a UINTEGER the compiler knows the shape behind |
| `p->x`, `p->next->x`, `f(3)->x`, `ptrs(i)->x` | a field through a pointer |

A pointer can be set to anything: `p = @a(3)`, `p = FlRec(id, i)`, or
`p = 49152` for a header that was loaded there. Pointers to different TYPEs
are not told apart.

A SUB takes a record through a pointer, and the call passes its address:

```basic
SUB Hurt(e AS Enemy PTR, by AS UBYTE)
    e->hp = e->hp - by
END SUB

Hurt(@foes(i), 2)
Hurt(@player, 1)
```

## What it compiles to

The records are lowered to plain BASIC before the parser reads the program:

| Source | Lowered |
|---|---|
| `DIM v AS T` | `DIM v(0 TO SIZE-1) AS UBYTE` |
| `DIM a(7) AS T` | `DIM a(0 TO 7, 0 TO SIZE-1) AS UBYTE` (every lower bound written out, so `array_base` is applied once, here) |
| `v.x` (global) | `PEEK(UBYTE, @v(0) + 2)` |
| `v.x` (local) | `PEEK(UBYTE, @v(2))` |
| `a(i).x` (global) | `PEEK(UBYTE, @a(0, 0) + 2 + (CAST(UINTEGER, i)) * SIZE)` |
| `a(i).x` (local) | `PEEK(UBYTE, @a(i, 2))` |
| `p->x` | `PEEK(UBYTE, p + 2)` |
| `v.x = e` | `POKE UBYTE @v(0) + 2, e` |
| `T PTR` | `UINTEGER` (a parameter is also `BYVAL`) |

`ZXR_DUMP_RECORDS=FILE` in the environment writes the lowered program to
`FILE`. The lowered program has the lines of the source, so every message
points at the line you wrote.

## What it costs

- **A field of a global record** is a fixed address, as cheap as a plain
  variable: `v.x = 1` is `ld a, 1 : ld (_v.__DATA__ + 2), a`.
- **A field of an element of a global array** is the array's address plus
  the index times the record's size: a multiplication by a constant each
  time it is used (a shift when the size is a power of two), and nothing at
  all when the index is a constant. The compiler's array routine, which a
  second dimension would make multiply in a loop, is not used. In a loop
  that touches several fields, take a pointer once:

  ```basic
  p = @foes(i)
  p->x = p->x + p->dx : p->y = p->y + p->dy
  ```

  `p->x` is the pointer in HL, the offset added to it, and one load.
- **A local array of records** goes through the array routine (a call to
  `__ARRAY`) for every field used.
- **A local record** lives on the heap like any local array.

## Not supported

- **No STRING fields.** A BASIC string is a pointer to the heap, and copying
  one safely needs the string runtime. Hold the address in a UINTEGER.
- **No copying a whole record** (`a = b`), and no initial value in its DIM.
  Assign the fields, or copy the bytes with `MemCopy(@b, @a, SIZEOF(T))`.
- **No methods** (`DECLARE SUB` inside a TYPE).
- **A record is not passed by value.** Pass it as a `T PTR`.
- **A field is not a FOR variable.**
- **No `->` after an expression in brackets** (`(p + 7)->x` is a syntax
  error). Assign it to a pointer first.
- **No debugger names for fields.** The `.sld` names a global record, not
  `v.x`.

Each of these is an error, not wrong code.

## Errors the compiler fixed on the way

Two bugs the Python compiler has turned up while this was being built. The
port fixes both (see [quirks.md](quirks.md)):

- PEEK(FIXED, …), PEEK(FLOAT, …) and POKE FLOAT at a constant address
  (`@g(0) + 16`) popped a word nobody pushed (`tests/fixes/const_address.bas`).
- Arithmetic on the address of a local array's element (`@r(1) + 1`) stopped
  with "Initializer expression is not constant"
  (`tests/fixes/local_address.bas`).
