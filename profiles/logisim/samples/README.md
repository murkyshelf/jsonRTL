# Drop Logisim samples here

Export from Logisim or Logisim Evolution and save the `.circ` files in this
folder. Useful set:

1. `basic.circ`      — a few gates wired to input/output pins (no subcircuits)
2. `subcircuit.circ` — a circuit that instantiates another circuit
3. `wide.circ`       — anything using multi-bit pins, tunnels or splitters

These calibrate the port-geometry table in `geometry.rs`, which is the one part
of the converter that cannot be verified without real files.
