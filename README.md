# lau-wasm-bridge

A zero-dependency WASM compilation target for **PLATO** core types. Provides world simulation primitives — vibe fields, agents, rooms, and a full world hierarchy — with hand-rolled binary serialization designed for `wasm32-unknown-unknown`.

---

## What This Does

`lau-wasm-bridge` is the data layer you compile to WebAssembly when the Lau/PLATO game engine needs to run inside a browser or embedded WASM runtime. It ships four core types:

- **WasmVibeField** — A growable array of `f64` values with conservation-law tracking. Set a baseline, fill values, then check whether `sum()` deviates from the baseline by more than a tolerance.
- **WasmAgent** — An entity with an internal state vector, sensor inputs, and actuator outputs. A reactive `tick(vibe)` computes `actuators[0] = vibe × state[0]`.
- **WasmRoom** — A container for agents with a shared `vibe` value and an energy budget. `room_tick()` advances all agents and tracks conservation error against the budget.
- **WasmWorld** — The top-level container of rooms. `world_tick()` advances every room, and `global_conservation_error()` measures total actuator output vs. total energy budget.

Plus manual **binary serialization** (`serialize_field`/`deserialize_field`, `serialize_world`/`deserialize_world`) — no serde, no allocator surprises, just little-endian byte slices.

---

## Key Idea

Every simulation step preserves an **energy conservation invariant**: the sum of all agent actuator outputs should stay close to the total energy budget across all rooms. This makes the simulation physically grounded — energy doesn't appear from nowhere or vanish into nothing. The conservation error functions give you a continuous diagnostic of how well the invariant holds.

The binary format is bespoke and deterministic: same data → same bytes, every time. No schema negotiation, no version headers. This is intentional for WASM where you want minimal overhead and maximum control.

---

## Install

```toml
[dependencies]
lau-wasm-bridge = "0.1"
```

Requires **Rust 2021 edition**. No external dependencies — pure `std` only.

### Building for WASM

```bash
rustup target add wasm32-unknown-unknown
cargo build --target wasm32-unknown-unknown
```

The crate type is `["lib", "cdylib"]`, so you get both a Rust library and a C-compatible dynamic library (the `.wasm` file).

---

## Quick Start

```rust
use lau_wasm_bridge::*;

// --- Vibe field ---
let mut field = WasmVibeField::new(10.0);
field.resize(3);
field.set(0, 3.0);
field.set(1, 4.0);
field.set(2, 3.0);
assert!(field.is_conserved(0.01)); // sum=10, baseline=10

// --- Agent ---
let mut agent = WasmAgent::new(1, 2, 2); // id=1, 2 sensors, 2 actuators
agent.state[0] = 3.0;
agent.tick(2.5); // actuators[0] = 2.5 * 3.0 = 7.5

// --- Room ---
let mut room = WasmRoom::new(42, 100.0); // id=42, budget=100
room.vibe = 1.5;
room.add_agent(WasmAgent::new(10, 1, 1));
room.room_tick();

// --- World ---
let mut world = WasmWorld::new();
world.add_room(room);
world.world_tick();
println!("rooms={}, energy={}", world.room_count(), world.total_energy());

// --- Serialize to bytes ---
let bytes = serialize_world(&world);
let restored = deserialize_world(&bytes).unwrap();
assert_eq!(restored.tick, world.tick);
```

---

## API Reference

### `WasmVibeField`

| method | description |
|--------|-------------|
| `new(baseline)` | Empty field with a conservation baseline. |
| `resize(n)` | Grow/shrink the value array (new slots = 0.0). |
| `set(i, v)` / `get(i)` | Index into values. Out-of-bounds `set` is a no-op; `get` returns 0.0. |
| `sum()` → `f64` | Sum of all values. |
| `conservation_error()` → `f64` | Relative error: `abs(sum - baseline) / abs(baseline)`. Absolute if baseline = 0. |
| `is_conserved(epsilon)` → `bool` | `conservation_error() < epsilon`. |
| `len()` / `is_empty()` | Standard collection queries. |

### `WasmAgent`

| method | description |
|--------|-------------|
| `new(id, sensor_count, actuator_count)` | State initialised to `[1.0]`. Sensors/actuators zeroed. |
| `read_sensor(ch)` / `write_actuator(ch, v)` | Accessor/mutator. Out-of-bounds reads return 0.0; writes are no-ops. |
| `tick(vibe)` | Reactive step: `actuators[0] = vibe * state[0]`. No-op if no actuators. |

### `WasmRoom`

| method | description |
|--------|-------------|
| `new(id, energy_budget)` | Empty room, vibe = 0.0. |
| `add_agent(agent)` / `remove_agent(id)` | Manage agents. |
| `room_tick()` | Tick all agents with the room's vibe, increment `tick_count`. |
| `total_vibe()` → `f64` | Returns the room's vibe value. |
| `agent_count()` → `usize` | Number of agents. |
| `conservation_error()` → `f64` | Sum of all actuator outputs vs. energy budget (relative). |

### `WasmWorld`

| method | description |
|--------|-------------|
| `new()` / `default()` | Empty world. |
| `add_room(room)` | Append a room. |
| `world_tick()` | Tick every room, increment world tick. |
| `total_energy()` → `f64` | Sum of all room energy budgets. |
| `room_count()` → `usize` | Number of rooms. |
| `global_conservation_error()` → `f64` | Total actuator output vs. total energy budget (relative). |

### Serialization Functions

```rust
// VibeField: [4B len u32] [len×8B f64] [4B tick u32] [8B baseline f64]
pub fn serialize_field(field: &WasmVibeField) -> Vec<u8>;
pub fn deserialize_field(data: &[u8]) -> Result<WasmVibeField, String>;

// World: nested structure (see "The Math" for full layout)
pub fn serialize_world(world: &WasmWorld) -> Vec<u8>;
pub fn deserialize_world(data: &[u8]) -> Result<WasmWorld, String>;
```

All multi-byte values are **little-endian**. No compression, no varint encoding — fixed-width for deterministic sizing.

---

## How It Works

```
┌─────────────────────────────────────────────────┐
│                   WasmWorld                      │
│  tick: u32                                       │
│  rooms: Vec<WasmRoom>                            │
│  ┌─────────────────────────────────────────────┐ │
│  │              WasmRoom                        │ │
│  │  id, vibe, tick_count, energy_budget         │ │
│  │  agents: Vec<WasmAgent>                      │ │
│  │  ┌─────────────────────────────────────────┐ │ │
│  │  │           WasmAgent                      │ │ │
│  │  │  id, state: Vec<f64>                     │ │ │
│  │  │  sensors: Vec<f64>  actuators: Vec<f64>  │ │ │
│  │  └─────────────────────────────────────────┘ │ │
│  └─────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────┘

world_tick()
  → for each room: room_tick()
    → for each agent: agent.tick(room.vibe)
      → actuators[0] = vibe * state[0]
```

The **conservation law** threads through the hierarchy:

1. Each agent produces actuator output.
2. Each room sums its agents' actuator outputs and compares to its `energy_budget`.
3. The world sums everything globally and compares to `total_energy()`.

The `WasmVibeField` applies the same principle at a lower level — a flat array of values that should sum to a baseline.

---

## The Math

### Conservation Error

For any level (field, room, or world), the conservation error is:

$$\varepsilon = \frac{|S - B|}{|B|}$$

where $S$ = sum of outputs (or values) and $B$ = baseline (or energy budget). When $B = 0$, the absolute error is used:

$$\varepsilon = |S|$$

A system is **conserved** when $\varepsilon < \epsilon_{\text{tol}}$ for some small tolerance (e.g. 0.01).

### Agent Reactive Rule

$$a_0^{(t+1)} = v \cdot s_0$$

where $v$ is the room vibe, $s_0$ is the agent's first state element, and $a_0$ is the first actuator. This is a linear scalar multiplication — intentionally simple for deterministic, reproducible behaviour across native and WASM targets.

### Binary Layout: `WasmVibeField`

```
┌──────────┬──────────────────┬──────────┬──────────┐
│ len: u32 │ len × f64 values │ tick:u32 │ base:f64 │
│  4 bytes │  len × 8 bytes   │  4 bytes │  8 bytes │
└──────────┴──────────────────┴──────────┴──────────┘
```

Total size: `4 + 8·len + 4 + 8 = 16 + 8·len` bytes.

### Binary Layout: `WasmWorld`

```
┌───────────┬──────────────┐
│ tick: u32 │ room_cnt:u32 │
└───────────┴──────────────┘
For each room:
┌──────┬──────────┬────────────┬────────────┬─────────────┐
│ id   │ vibe:f64 │ tick_cnt   │ energy:f64 │ agent_cnt   │
│ u32  │  8 bytes │ u32        │  8 bytes   │ u32         │
└──────┴──────────┴────────────┴────────────┴─────────────┘
  For each agent:
  ┌──────┬────────────┬─────────────────┬────────────┬───────────────┐
  │ id   │ state_len  │ state_len × f64 │ sensor_cnt │ actuator_cnt  │
  │ u32  │ u32        │                 │ u32        │ u32           │
  └──────┴────────────┴─────────────────┴────────────┴───────────────┘
  ┌───────────────────────┬──────────────────────────┐
  │ sensor_cnt × f64      │ actuator_cnt × f64       │
  └───────────────────────┴──────────────────────────┘
```

All multi-byte fields are little-endian (`to_le_bytes` / `from_le_bytes`). No alignment padding — fields are packed sequentially.

---

## Tests

**29 tests** covering:

- `WasmVibeField`: creation, set/get, sum, conservation error (zero and non-zero baseline), is_conserved, resize, out-of-bounds safety
- `WasmAgent`: creation, sensor/actuator I/O, reactive tick, no-actuator edge case, out-of-bounds reads
- `WasmRoom`: creation, add/remove agents, room_tick propagation, conservation error, total_vibe
- `WasmWorld`: creation, default, add_room + world_tick, total_energy, global conservation error
- Serialization: field round-trip (empty and populated), world round-trip (empty, single room, multi-room), truncated-data error handling

```bash
cargo test
```

---

## License

MIT
