//! `lau-wasm-bridge` — WASM compilation target for PLATO core types.
//!
//! Designed for `wasm32-unknown-unknown` but compiles and tests on native first.
//! No external dependencies; pure `std` only.

// ── WasmVibeField ──────────────────────────────────────────────────────────

/// A field of vibe values with conservation-law tracking.
#[derive(Debug, Clone)]
pub struct WasmVibeField {
    pub values: Vec<f64>,
    pub tick: u32,
    pub conservation_baseline: f64,
}

impl WasmVibeField {
    pub fn new(baseline: f64) -> Self {
        Self {
            values: Vec::new(),
            tick: 0,
            conservation_baseline: baseline,
        }
    }

    pub fn set(&mut self, index: usize, value: f64) {
        if index < self.values.len() {
            self.values[index] = value;
        }
    }

    pub fn get(&self, index: usize) -> f64 {
        self.values.get(index).copied().unwrap_or(0.0)
    }

    pub fn sum(&self) -> f64 {
        self.values.iter().copied().sum()
    }

    pub fn conservation_error(&self) -> f64 {
        let baseline = self.conservation_baseline;
        if baseline == 0.0 {
            return (self.sum() - baseline).abs();
        }
        (self.sum() - baseline).abs() / baseline.abs()
    }

    pub fn is_conserved(&self, epsilon: f64) -> bool {
        self.conservation_error() < epsilon
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn resize(&mut self, new_len: usize) {
        self.values.resize(new_len, 0.0);
    }
}

// ── WasmAgent ──────────────────────────────────────────────────────────────

/// An agent inside a room, with sensors, actuators, and reactive behaviour.
#[derive(Debug, Clone)]
pub struct WasmAgent {
    pub id: u32,
    pub state: Vec<f64>,
    pub sensor_count: u32,
    pub actuator_count: u32,
    pub sensors: Vec<f64>,
    pub actuators: Vec<f64>,
}

impl WasmAgent {
    pub fn new(id: u32, sensor_count: u32, actuator_count: u32) -> Self {
        Self {
            id,
            state: vec![1.0],
            sensor_count,
            actuator_count,
            sensors: vec![0.0; sensor_count as usize],
            actuators: vec![0.0; actuator_count as usize],
        }
    }

    pub fn read_sensor(&self, channel: u32) -> f64 {
        self.sensors.get(channel as usize).copied().unwrap_or(0.0)
    }

    pub fn write_actuator(&mut self, channel: u32, value: f64) {
        let idx = channel as usize;
        if idx < self.actuators.len() {
            self.actuators[idx] = value;
        }
    }

    /// Simple reactive tick: `actuators[0] = vibe * state[0]`.
    pub fn tick(&mut self, vibe: f64) {
        let s = self.state.first().copied().unwrap_or(1.0);
        if !self.actuators.is_empty() {
            self.actuators[0] = vibe * s;
        }
    }
}

// ── WasmRoom ───────────────────────────────────────────────────────────────

/// A room containing agents and a vibe value.
#[derive(Debug, Clone)]
pub struct WasmRoom {
    pub id: u32,
    pub vibe: f64,
    pub agents: Vec<WasmAgent>,
    pub tick_count: u32,
    pub energy_budget: f64,
}

impl WasmRoom {
    pub fn new(id: u32, energy_budget: f64) -> Self {
        Self {
            id,
            vibe: 0.0,
            agents: Vec::new(),
            tick_count: 0,
            energy_budget,
        }
    }

    pub fn add_agent(&mut self, agent: WasmAgent) {
        self.agents.push(agent);
    }

    pub fn remove_agent(&mut self, agent_id: u32) {
        self.agents.retain(|a| a.id != agent_id);
    }

    /// Tick all agents, compute conservation.
    pub fn room_tick(&mut self) {
        let vibe = self.vibe;
        for agent in &mut self.agents {
            agent.tick(vibe);
        }
        self.tick_count += 1;
    }

    pub fn total_vibe(&self) -> f64 {
        self.vibe
    }

    pub fn agent_count(&self) -> usize {
        self.agents.len()
    }

    pub fn conservation_error(&self) -> f64 {
        let total: f64 = self.agents.iter().map(|a| a.actuators.iter().copied().sum::<f64>()).sum();
        if self.energy_budget == 0.0 {
            return total.abs();
        }
        (total - self.energy_budget).abs() / self.energy_budget.abs()
    }
}

// ── WasmWorld ──────────────────────────────────────────────────────────────

/// Top-level world containing rooms.
#[derive(Debug, Clone)]
pub struct WasmWorld {
    pub rooms: Vec<WasmRoom>,
    pub tick: u32,
}

impl WasmWorld {
    pub fn new() -> Self {
        Self {
            rooms: Vec::new(),
            tick: 0,
        }
    }

    pub fn add_room(&mut self, room: WasmRoom) {
        self.rooms.push(room);
    }

    /// Tick all rooms.
    pub fn world_tick(&mut self) {
        for room in &mut self.rooms {
            room.room_tick();
        }
        self.tick += 1;
    }

    pub fn total_energy(&self) -> f64 {
        self.rooms.iter().map(|r| r.energy_budget).sum()
    }

    pub fn room_count(&self) -> usize {
        self.rooms.len()
    }

    pub fn global_conservation_error(&self) -> f64 {
        let budget: f64 = self.total_energy();
        let total: f64 = self
            .rooms
            .iter()
            .flat_map(|r| r.agents.iter())
            .flat_map(|a| a.actuators.iter().copied())
            .sum();
        if budget == 0.0 {
            return total.abs();
        }
        (total - budget).abs() / budget.abs()
    }
}

impl Default for WasmWorld {
    fn default() -> Self {
        Self::new()
    }
}

// ── Serialization (manual, no serde) ──────────────────────────────────────

/// Binary format layout for `WasmVibeField`:
///
/// ```text
/// [4B len as u32] [len × 8B f64 values] [4B tick as u32] [8B baseline as f64]
/// ```
pub fn serialize_field(field: &WasmVibeField) -> Vec<u8> {
    let len = field.values.len() as u32;
    let mut buf = Vec::with_capacity(4 + (len as usize) * 8 + 4 + 8);
    buf.extend_from_slice(&len.to_le_bytes());
    for v in &field.values {
        buf.extend_from_slice(&v.to_le_bytes());
    }
    buf.extend_from_slice(&field.tick.to_le_bytes());
    buf.extend_from_slice(&field.conservation_baseline.to_le_bytes());
    buf
}

pub fn deserialize_field(data: &[u8]) -> Result<WasmVibeField, String> {
    if data.len() < 4 {
        return Err("field data too short (need at least 4 bytes for length)".into());
    }
    let len = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
    let need = 4 + len * 8 + 4 + 8;
    if data.len() < need {
        return Err(format!(
            "field data too short: got {} bytes, need {}",
            data.len(), need
        ));
    }
    let mut off = 4;
    let mut values = Vec::with_capacity(len);
    for _ in 0..len {
        let v = f64::from_le_bytes(data[off..off + 8].try_into().unwrap());
        values.push(v);
        off += 8;
    }
    let tick = u32::from_le_bytes(data[off..off + 4].try_into().unwrap());
    off += 4;
    let baseline = f64::from_le_bytes(data[off..off + 8].try_into().unwrap());

    Ok(WasmVibeField {
        values,
        tick,
        conservation_baseline: baseline,
    })
}

/// Binary format for `WasmWorld`:
///
/// ```text
/// [4B world_tick] [4B room_count]
/// for each room:
///   [4B id] [8B vibe] [4B tick_count] [8B energy_budget] [4B agent_count]
///   for each agent:
///     [4B id] [4B state_len] [state_len × 8B] [4B sensor_count] [4B actuator_count]
///     [sensor_count × 8B sensors] [actuator_count × 8B actuators]
/// ```
pub fn serialize_world(world: &WasmWorld) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&world.tick.to_le_bytes());
    buf.extend_from_slice(&(world.rooms.len() as u32).to_le_bytes());

    for room in &world.rooms {
        buf.extend_from_slice(&room.id.to_le_bytes());
        buf.extend_from_slice(&room.vibe.to_le_bytes());
        buf.extend_from_slice(&room.tick_count.to_le_bytes());
        buf.extend_from_slice(&room.energy_budget.to_le_bytes());
        buf.extend_from_slice(&(room.agents.len() as u32).to_le_bytes());

        for agent in &room.agents {
            buf.extend_from_slice(&agent.id.to_le_bytes());
            buf.extend_from_slice(&(agent.state.len() as u32).to_le_bytes());
            for v in &agent.state {
                buf.extend_from_slice(&v.to_le_bytes());
            }
            buf.extend_from_slice(&agent.sensor_count.to_le_bytes());
            buf.extend_from_slice(&agent.actuator_count.to_le_bytes());
            for v in &agent.sensors {
                buf.extend_from_slice(&v.to_le_bytes());
            }
            for v in &agent.actuators {
                buf.extend_from_slice(&v.to_le_bytes());
            }
        }
    }
    buf
}

pub fn deserialize_world(data: &[u8]) -> Result<WasmWorld, String> {
    let mut off = 0usize;

    fn read_u32(data: &[u8], off: &mut usize) -> Result<u32, String> {
        if data.len() < *off + 4 {
            return Err("unexpected end of data reading u32".into());
        }
        let v = u32::from_le_bytes(data[*off..*off + 4].try_into().unwrap());
        *off += 4;
        Ok(v)
    }

    fn read_f64(data: &[u8], off: &mut usize) -> Result<f64, String> {
        if data.len() < *off + 8 {
            return Err("unexpected end of data reading f64".into());
        }
        let v = f64::from_le_bytes(data[*off..*off + 8].try_into().unwrap());
        *off += 8;
        Ok(v)
    }

    fn read_f64_vec(data: &[u8], off: &mut usize, len: usize) -> Result<Vec<f64>, String> {
        let mut v = Vec::with_capacity(len);
        for _ in 0..len {
            v.push(read_f64(data, off)?);
        }
        Ok(v)
    }

    let tick = read_u32(data, &mut off)?;
    let room_count = read_u32(data, &mut off)? as usize;
    let mut rooms = Vec::with_capacity(room_count);

    for _ in 0..room_count {
        let id = read_u32(data, &mut off)?;
        let vibe = read_f64(data, &mut off)?;
        let tick_count = read_u32(data, &mut off)?;
        let energy_budget = read_f64(data, &mut off)?;
        let agent_count = read_u32(data, &mut off)? as usize;
        let mut agents = Vec::with_capacity(agent_count);

        for _ in 0..agent_count {
            let aid = read_u32(data, &mut off)?;
            let state_len = read_u32(data, &mut off)? as usize;
            let state = read_f64_vec(data, &mut off, state_len)?;
            let sensor_count = read_u32(data, &mut off)?;
            let actuator_count = read_u32(data, &mut off)?;
            let sensors = read_f64_vec(data, &mut off, sensor_count as usize)?;
            let actuators = read_f64_vec(data, &mut off, actuator_count as usize)?;

            agents.push(WasmAgent {
                id: aid,
                state,
                sensor_count,
                actuator_count,
                sensors,
                actuators,
            });
        }

        rooms.push(WasmRoom {
            id,
            vibe,
            agents,
            tick_count,
            energy_budget,
        });
    }

    Ok(WasmWorld { rooms, tick })
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── WasmVibeField tests ──

    #[test]
    fn field_new_empty() {
        let f = WasmVibeField::new(10.0);
        assert_eq!(f.values.len(), 0);
        assert_eq!(f.tick, 0);
        assert_eq!(f.conservation_baseline, 10.0);
    }

    #[test]
    fn field_set_get_sum() {
        let mut f = WasmVibeField::new(10.0);
        f.resize(3);
        f.set(0, 3.0);
        f.set(1, 4.0);
        f.set(2, 5.0);
        assert_eq!(f.get(0), 3.0);
        assert_eq!(f.get(1), 4.0);
        assert_eq!(f.get(2), 5.0);
        assert_eq!(f.sum(), 12.0);
    }

    #[test]
    fn field_get_out_of_bounds() {
        let f = WasmVibeField::new(0.0);
        assert_eq!(f.get(999), 0.0);
    }

    #[test]
    fn field_conservation_error_zero_baseline() {
        let mut f = WasmVibeField::new(0.0);
        f.resize(2);
        f.set(0, 3.0);
        f.set(1, 4.0);
        assert_eq!(f.conservation_error(), 7.0);
    }

    #[test]
    fn field_conservation_error_nonzero() {
        let mut f = WasmVibeField::new(12.0);
        f.resize(3);
        f.set(0, 3.0);
        f.set(1, 4.0);
        f.set(2, 5.0);
        // sum=12, baseline=12, error=0
        assert!((f.conservation_error()).abs() < 1e-10);
    }

    #[test]
    fn field_is_conserved() {
        let mut f = WasmVibeField::new(12.0);
        f.resize(3);
        f.set(0, 3.0);
        f.set(1, 4.0);
        f.set(2, 5.0);
        assert!(f.is_conserved(0.01));
        f.set(0, 100.0);
        assert!(!f.is_conserved(0.01));
    }

    #[test]
    fn field_resize_grows_and_shrinks() {
        let mut f = WasmVibeField::new(0.0);
        f.resize(5);
        assert_eq!(f.len(), 5);
        f.resize(2);
        assert_eq!(f.len(), 2);
    }

    #[test]
    fn field_set_out_of_bounds_noop() {
        let mut f = WasmVibeField::new(0.0);
        f.resize(1);
        f.set(5, 42.0); // should not panic
        assert_eq!(f.get(5), 0.0);
    }

    // ── WasmAgent tests ──

    #[test]
    fn agent_new() {
        let a = WasmAgent::new(1, 3, 2);
        assert_eq!(a.id, 1);
        assert_eq!(a.sensors.len(), 3);
        assert_eq!(a.actuators.len(), 2);
        assert_eq!(a.state, vec![1.0]);
    }

    #[test]
    fn agent_read_write_sensor_actuator() {
        let mut a = WasmAgent::new(0, 2, 2);
        a.write_actuator(0, 7.5);
        assert_eq!(a.read_sensor(0), 0.0); // sensors still zero
        assert!((a.actuators[0] - 7.5).abs() < 1e-10);
    }

    #[test]
    fn agent_tick_reactive() {
        let mut a = WasmAgent::new(0, 1, 2);
        a.state[0] = 2.0;
        a.tick(3.5);
        assert!((a.actuators[0] - 7.0).abs() < 1e-10);
    }

    #[test]
    fn agent_tick_no_actuators_no_panic() {
        let mut a = WasmAgent::new(0, 0, 0);
        a.tick(99.0); // should not panic
    }

    #[test]
    fn agent_read_sensor_out_of_bounds() {
        let a = WasmAgent::new(0, 1, 1);
        assert_eq!(a.read_sensor(99), 0.0);
    }

    // ── WasmRoom tests ──

    #[test]
    fn room_new() {
        let r = WasmRoom::new(1, 100.0);
        assert_eq!(r.id, 1);
        assert_eq!(r.energy_budget, 100.0);
        assert_eq!(r.agent_count(), 0);
    }

    #[test]
    fn room_add_remove_agent() {
        let mut r = WasmRoom::new(1, 100.0);
        r.add_agent(WasmAgent::new(10, 1, 1));
        r.add_agent(WasmAgent::new(20, 1, 1));
        assert_eq!(r.agent_count(), 2);
        r.remove_agent(10);
        assert_eq!(r.agent_count(), 1);
        assert_eq!(r.agents[0].id, 20);
    }

    #[test]
    fn room_tick_updates_agents() {
        let mut r = WasmRoom::new(1, 0.0);
        r.vibe = 5.0;
        r.add_agent(WasmAgent::new(0, 1, 1));
        r.room_tick();
        assert_eq!(r.tick_count, 1);
        assert!((r.agents[0].actuators[0] - 5.0).abs() < 1e-10);
    }

    #[test]
    fn room_conservation_error() {
        let mut r = WasmRoom::new(1, 10.0);
        r.vibe = 5.0;
        let mut a = WasmAgent::new(0, 1, 1);
        a.state[0] = 2.0;
        r.add_agent(a);
        r.room_tick();
        // actuators[0] = 5*2 = 10, budget = 10, error ≈ 0
        assert!(r.conservation_error() < 0.01);
    }

    #[test]
    fn room_total_vibe() {
        let r = WasmRoom {
            id: 1,
            vibe: 42.0,
            agents: vec![],
            tick_count: 0,
            energy_budget: 0.0,
        };
        assert_eq!(r.total_vibe(), 42.0);
    }

    // ── WasmWorld tests ──

    #[test]
    fn world_new() {
        let w = WasmWorld::new();
        assert_eq!(w.room_count(), 0);
        assert_eq!(w.tick, 0);
    }

    #[test]
    fn world_default() {
        let w = WasmWorld::default();
        assert_eq!(w.room_count(), 0);
    }

    #[test]
    fn world_add_room_and_tick() {
        let mut w = WasmWorld::new();
        let mut r = WasmRoom::new(1, 100.0);
        r.vibe = 2.0;
        r.add_agent(WasmAgent::new(0, 1, 1));
        w.add_room(r);
        w.world_tick();
        assert_eq!(w.tick, 1);
        assert_eq!(w.room_count(), 1);
        assert!((w.rooms[0].agents[0].actuators[0] - 2.0).abs() < 1e-10);
    }

    #[test]
    fn world_total_energy() {
        let mut w = WasmWorld::new();
        w.add_room(WasmRoom::new(1, 50.0));
        w.add_room(WasmRoom::new(2, 30.0));
        assert!((w.total_energy() - 80.0).abs() < 1e-10);
    }

    #[test]
    fn world_global_conservation_error() {
        let mut w = WasmWorld::new();
        let mut r = WasmRoom::new(1, 6.0);
        r.vibe = 6.0;
        r.add_agent(WasmAgent::new(0, 1, 1));
        w.add_room(r);
        w.world_tick();
        // actuators[0] = 6*1 = 6, total budget = 6
        assert!(w.global_conservation_error() < 0.01);
    }

    // ── Serialization tests ──

    #[test]
    fn serialize_deserialize_field_roundtrip() {
        let mut f = WasmVibeField::new(42.5);
        f.resize(3);
        f.set(0, 1.0);
        f.set(1, 2.0);
        f.set(2, 3.0);
        f.tick = 7;
        let buf = serialize_field(&f);
        let f2 = deserialize_field(&buf).unwrap();
        assert_eq!(f2.values, f.values);
        assert_eq!(f2.tick, f.tick);
        assert!((f2.conservation_baseline - 42.5).abs() < 1e-10);
    }

    #[test]
    fn serialize_deserialize_field_empty() {
        let f = WasmVibeField::new(0.0);
        let buf = serialize_field(&f);
        let f2 = deserialize_field(&buf).unwrap();
        assert!(f2.values.is_empty());
    }

    #[test]
    fn deserialize_field_too_short() {
        let result = deserialize_field(&[0, 0]);
        assert!(result.is_err());
    }

    #[test]
    fn serialize_deserialize_world_roundtrip() {
        let mut w = WasmWorld::new();
        let mut r = WasmRoom::new(1, 50.0);
        r.vibe = 3.0;
        let mut a = WasmAgent::new(10, 2, 3);
        a.state = vec![1.0, 2.0];
        a.sensors = vec![0.5, 0.6];
        a.actuators = vec![1.0, 2.0, 3.0];
        r.add_agent(a);
        w.add_room(r);
        w.tick = 5;

        let buf = serialize_world(&w);
        let w2 = deserialize_world(&buf).unwrap();

        assert_eq!(w2.tick, 5);
        assert_eq!(w2.room_count(), 1);
        assert_eq!(w2.rooms[0].id, 1);
        assert!((w2.rooms[0].vibe - 3.0).abs() < 1e-10);
        assert_eq!(w2.rooms[0].agents.len(), 1);
        assert_eq!(w2.rooms[0].agents[0].id, 10);
        assert_eq!(w2.rooms[0].agents[0].state, vec![1.0, 2.0]);
        assert_eq!(w2.rooms[0].agents[0].sensors, vec![0.5, 0.6]);
        assert_eq!(w2.rooms[0].agents[0].actuators, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn deserialize_world_empty() {
        let w = WasmWorld::new();
        let buf = serialize_world(&w);
        let w2 = deserialize_world(&buf).unwrap();
        assert_eq!(w2.room_count(), 0);
        assert_eq!(w2.tick, 0);
    }

    #[test]
    fn serialize_deserialize_world_multiple_rooms() {
        let mut w = WasmWorld::new();
        let mut r1 = WasmRoom::new(1, 10.0);
        r1.vibe = 1.0;
        r1.add_agent(WasmAgent::new(0, 1, 1));
        let mut r2 = WasmRoom::new(2, 20.0);
        r2.vibe = 2.0;
        r2.add_agent(WasmAgent::new(1, 2, 2));
        w.add_room(r1);
        w.add_room(r2);
        w.tick = 3;

        let buf = serialize_world(&w);
        let w2 = deserialize_world(&buf).unwrap();
        assert_eq!(w2.room_count(), 2);
        assert_eq!(w2.tick, 3);
        assert_eq!(w2.rooms[0].agents.len(), 1);
        assert_eq!(w2.rooms[1].agents.len(), 1);
        assert_eq!(w2.rooms[1].agents[0].sensor_count, 2);
        assert_eq!(w2.rooms[1].agents[0].actuator_count, 2);
    }
}
