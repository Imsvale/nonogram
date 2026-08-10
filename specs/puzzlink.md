# puzz.link Nonogram URL Format

puzz.link is a web-based puzzle sharing platform. A nonogram can be encoded
directly in the URL, making it shareable and openable without any server-side
storage.

URL template:

```
https://puzz.link/p?nonogram/{W}/{H}/{data}
```

where `W` is the grid width (number of columns), `H` is the grid height (number
of rows), and `{data}` is the encoded clue string described below.

---

## Encoding overview

The `{data}` string encodes all column clue groups followed by all row clue
groups, in order (column 1 through W, then row 1 through H).

Each group is encoded as a sequence of clue values (in **reverse order**,
last value first) followed by an optional **padding character** that brings the
group to a fixed length. The padding character is omitted when all G slots are
filled (i.e. `zeros = 0`).

The fixed lengths are:

| Axis    | Group size G       |
|---------|--------------------|
| Columns | `ceil(H / 2)`      |
| Rows    | `ceil(W / 2)`      |

These are the maximum possible numbers of filled segments per line of that
length.

---

## Value encoding

Individual clue values within a group are encoded as follows:

| Value range | Encoding                               | Examples              |
|-------------|----------------------------------------|-----------------------|
| 1 – 9       | ASCII digit                            | `1` `5` `9`           |
| 10 – 15     | Lowercase hex letter `a`–`f`           | `a`=10 `b`=11 `f`=15  |
| 16 and up   | `-` followed by two-digit lowercase hex | `-10`=16 `-1f`=31 `-20`=32 |

Value `0` is never used (clue lists are either empty or contain only positive
integers).

---

## Padding character

After all values in a group, a padding suffix is appended when there are
trailing zero slots (`zeros = G − clue_count > 0`). When `zeros = 0` (all G
slots filled with non-zero values), **no padding character is emitted**.

**Single-character form** (zeros 1–20):

```
padding = chr(ord('f') + zeros)
```

| zeros | character |
|-------|-----------|
| 1     | `g`       |
| 2     | `h`       |
| …     | …         |
| 19    | `y`       |
| 20    | `z`       |

**Multi-character form** (zeros > 20):

Each group of 20 zeros is encoded as one `z` character; any remainder (1–19)
is encoded as a single trailing suffix character as above. There is no upper
bound on the number of `z` characters.

```
padding = 'z' × (zeros ÷ 20)  +  (zeros mod 20 > 0 ? chr(ord('f') + zeros mod 20) : "")
```

| zeros | encoding |
|-------|----------|
| 21    | `zg`     |
| 25    | `zk`     |
| 40    | `zz`     |
| 41    | `zzg`    |
| 48    | `zzn`    |
| 50    | `zzp`    |
| 60    | `zzz`    |
| 61    | `zzzg`   |

This arises in practice for any line whose clue count is small relative to G.
For example, in a 100×100 grid G\_col = 50; an empty column has 50 padding
zeros and encodes its suffix as `zzp`.

The characters `g`–`z` (and the `z?` two-character forms) are unambiguous
padding because `a`–`f` are reserved for clue values 10–15 and `0`–`9` for
values 0–9; a character ≥ `g` at the expected suffix position can only be
padding.

An empty column or row (no clues) encodes as a padding suffix representing
G zeros.

---

## Decoding a group

To decode one group of size G, read characters until a padding character
(`≥ 'g'`) is encountered or all G value slots are filled:

1. While the collected value count is less than G and the next character is a
   digit or `a`–`f` or `-`:
   - If `-`: read the next two characters as a lowercase hex integer → value.
   - Otherwise: decode the single character as a value (digit → itself,
     `a`–`f` → 10–15).
   - Append the value to the group list.
2. If the next character is `≥ 'g'`, consume suffix characters to compute
   the zero count: each `z` contributes 20; the first non-`z` suffix character
   (if any) contributes `ord(c) − ord('f')` and terminates the suffix. If all
   G slots are already filled, no padding character is present — stop here
   without consuming any further input.
3. **Reverse** the collected value list to get the clue order (top-to-bottom
   for columns, left-to-right for rows).

---

## Worked example — "Eye" 30×15

```
https://puzz.link/p?nonogram/30/15/1m3m5m7m44l33l33l33l23l252k3122j21322i...
```

Parameters: W=30, H=15  
Group sizes: G\_col = ceil(15/2) = **8**, G\_row = ceil(30/2) = **15**

### Column groups (first 10 of 30)

| Group | Encoded | Decoded (reversed) | Clue       |
|-------|---------|--------------------|------------|
| col 1  | `1m`    | [1] + 7 zeros      | [1]        |
| col 2  | `3m`    | [3] + 7 zeros      | [3]        |
| col 3  | `5m`    | [5] + 7 zeros      | [5]        |
| col 4  | `7m`    | [7] + 7 zeros      | [7]        |
| col 5  | `44l`   | [4,4] + 6 zeros    | [4,4]      |
| col 6  | `33l`   | [3,3] + 6 zeros    | [3,3]      |
| col 9  | `23l`   | [2,3] reversed → [3,2] | [3,2]  |
| col 10 | `252k`  | [2,5,2] + 5 zeros  | [2,5,2]    |
| col 11 | `3122j` | [3,1,2,2] reversed → [2,2,1,3] | [2,2,1,3] |
| col 12 | `21322i`| [2,1,3,2,2] reversed → [2,2,3,1,2] | [2,2,3,1,2] |

### Row groups (first 3 of 15)

| Group | Encoded | Decoded (reversed) | Clue    |
|-------|---------|--------------------|---------|
| row 1  | `9t`    | [9] + 14 zeros     | [9]     |
| row 2  | `ft`    | [`f`=15] + 14 zeros | [15]   |
| row 3  | `554r`  | [5,5,4] reversed → [4,5,5] + 12 zeros | [4,5,5] |

---

## Worked example — "Boot" 20×15 (multi-digit values)

Group sizes: G\_col = **8**, G\_row = **10**

Selected groups showing values ≥ 10:

| Group  | Encoded  | Clue      | Notes                    |
|--------|----------|-----------|--------------------------|
| col 2  | `am`     | [10]      | `a` = 10                 |
| col 3  | `dm`     | [13]      | `d` = 13                 |
| col 13 | `b1l`    | [1,11]    | `b`=11, reversed [11,1]→[1,11] |
| col 14 | `a2l`    | [2,10]    | `a`=10, reversed [10,2]→[2,10] |
| row 11 | `1-101m` | [1,16,1]  | `-10` = hex 0x10 = 16    |
| row 12 | `2-11n`  | [17,2]    | `-11` = hex 0x11 = 17    |
| row 13 | `-13o`   | [19]      | `-13` = hex 0x13 = 19    |

---

## Notes

- **Clue order within a group is reversed** in the encoding: the last clue
  value appears first. Reversing after decoding restores the natural order.
- The format is compact: short groups waste proportionally more space (all
  padding), while long groups approach full density.
- The largest clue value representable with two hex digits is 255 (`-ff`).
  Grids wider or taller than 255 are not commonly encountered.
- The `{data}` string length depends on the actual clue counts: groups where
  all G slots are filled (zeros = 0) emit no padding character and are one
  character shorter than groups with trailing zeros. The maximum length is
  `W × G_col + H × G_row` (plus extra characters for values ≥ 10 or ≥ 16),
  reached only when every group has at least one trailing zero.
