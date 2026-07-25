# Phase M: multi-bit buses (canonical schema v1.1)

## Problem

The kernel already compiles nets wider than one bit — a width-8 `XOR` emits
`assign y = a ^ b;` over `[7:0]` operands. What it cannot express is a
connection to *part* of a net. Canonical v1.0 gives every component a single
uniform `width` and every connection is a bare net id, so there is no way to say
"bit 3 of net X".

Every real bus design needs exactly that. In Sebastian Lague's Digital-Logic-Sim
a bus is decomposed by explicit splitter and merger chips, and 23 of the 32
chips in the reference project use them. Without bit addressing the DLS profile
must reject any pin with `BitCount > 1`.

## The DLS bus model (decoded from the reference project)

Splitters and mergers are built-in chips named `X-YBIT`, meaning "convert X-bit
to Y-bit". They split when `X > Y` and merge when `X < Y`; the piece count is
`max(X,Y) / min(X,Y)`.

| Chip | Role | Input pins | Output pins |
| --- | --- | --- | --- |
| `8-1BIT` | split 8-bit into 8 bits | `0` | `1..8` |
| `1-8BIT` | merge 8 bits into 8-bit | `0..7` | `8` |
| `8-4BIT` | split 8-bit into two nibbles | `0` | `1..2` |
| `4-8BIT` | merge two nibbles into 8-bit | `0..1` | `2` |

Both orderings are **MSB first**: split output pin `k` (1-based) is bit
`N - k`; merge input pin `j` (0-based) is bit `N - 1 - j`. This was confirmed
against the carry chain in `16-BIT ADDER`, where the 4-bit adder that receives
`CIN` is fed from split pins `5..8` and drives merge pins `4..7`.

Two further built-ins are pure routing:

- `BUS-N` — a fan-out alias: pin `0` in, pin `1` out, same signal.
- `BUS-TERMINUS-N` — the far end of a drawn bus line. It has no output pins and
  drives nothing, so it is a sink that can be dropped.

## Design

### Canonical schema v1.1: sliced connections

A component connection becomes either a net id (as in v1.0) or a contiguous
slice of one:

```json
"connections": {
  "A": "net0",
  "B": { "net": "net1", "msb": 3, "lsb": 3 },
  "Y": { "net": "net2", "msb": 7, "lsb": 4 }
}
```

The typed model gains:

```rust
pub enum Connection { Whole(String), Slice(NetSlice) }
pub struct NetSlice { pub net: String, pub msb: u32, pub lsb: u32 }
```

`Connection` is `#[serde(untagged)]`, so every v1.0 document still parses
unchanged. Module ports keep whole-net `netId`; only components slice.

A slice's width is `msb - lsb + 1` and must equal the component's `width`, which
keeps the uniform-width rule intact per component. Non-contiguous gathering is
expressed by several components each driving a different slice of one net —
there is no concat expression, and no new component type is needed.

Rejected alternative: bit-blasting buses into scalar nets. It needs no kernel
change but makes a 16-bit adder expose 34 one-bit module ports instead of 8
buses, which is not usable Verilog for the designs this has to serve.

### Validation

The single-driver rule moves from the net to the **(net, bit)** pair, because
`assign d[0] = ...; assign d[1] = ...;` is legal and is precisely what a merger
compiles to.

| Rule | Granularity |
| --- | --- |
| `NET_MULTIPLE_DRIVERS` | per bit |
| `NET_NO_DRIVER` | per bit, raised only for bits that are consumed |
| `NET_UNUSED` (warning) | per net, when no bit is driven or consumed |
| `NET_NO_CONSUMERS` (warning) | per net, when some bit is driven and none consumed |
| `SLICE_OUT_OF_RANGE` (new) | `msb >= net.width` or `lsb > msb` |
| `SLICE_REQUIRES_SCHEMA_1_1` (new) | a slice appears in a `1.0` document |

Keeping the two warnings per-net means width-1 documents produce byte-identical
diagnostics to today.

Combinational-cycle detection also moves to (net, bit) nodes. This is strictly
more precise: a barrel shifter that routes bit 0 into one net and bit 1 out of
it is no longer a false cycle.

### Emission

`NormalizedConnection` carries the optional slice and renders itself as
`name`, `name[bit]`, or `name[msb:lsb]`. The emitter substitutes that
expression everywhere it currently uses the net identifier, on both sides of an
assignment. No change to statement ordering, so existing goldens are unaffected.

### DLS profile

Elaboration stops rejecting `BitCount > 1`. Splitter, merger, and `BUS-N` chips
are resolved during flattening rather than emitted as components:

- a split of net `S` binds each output pin to the slice `S[bit]`;
- a merge binds each input pin to the slice of its destination net;
- `BUS-N` unions its two pins into one net, exactly like a wire;
- `BUS-TERMINUS-N` is dropped.

Chips that mix widths on one wire are rejected with the existing `Unsupported`
diagnostic naming the chip and pin.

## Out of scope

Sequential elements. `1-bit reg` and the six chips built on it are NAND latches
and still fail with `GRAPH_COMBINATIONAL_CYCLE`, which is correct for a
combinational kernel. `3-STATE BUFFER` also stays rejected. Both are Phase S.

## Verification

- Unit tests per rule, with a v1.0 document proving diagnostics are unchanged.
- Byte-exact golden Verilog for a sliced circuit.
- End to end: `8bit and`, `8bit OR`, `8bit XOR`, `8-bit flipper`, `8BIT ENABLE`,
  `4X 8BIT OR`, `16-BIT ADDER`, and `16bit add-sub` compile from the reference
  project, and the 16-bit adder's sum is hand-checked against its carry chain.
