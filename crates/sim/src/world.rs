//! Truth world: entities as struct-of-arrays with optional components (DESIGN.md §4.4), so
//! non-tank entity types can be added without touching vehicle code (CLAUDE.md rule 5).

use ai::picture::{Condition, PlatoonId, WeaponsStatus};
use sim_data::{SensorKind, SmallArmDef, SquadDef, VehicleDef};
use terrain::hash::StableHasher;

use crate::Vec2;

pub use ai::picture::{EntityId, SideId};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    /// Local map metres (x east, y north).
    pub pos: Vec2,
    /// Ground elevation at `pos` (m).
    pub z: f64,
    /// Compass heading of the hull (rad, clockwise from north).
    pub heading: f64,
    /// Hull pitch (rad, nose up positive) and roll (rad, right side down positive).
    pub pitch: f64,
    pub roll: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MobilityState {
    /// Signed speed along the hull axis (m/s).
    pub speed: f64,
    pub fuel_l: f64,
    pub stalled_s: f64,
    /// Commanded by the vehicle AI.
    pub cmd_speed: f64,
    pub cmd_heading: f64,
    /// Commanded to drive backwards (cmd_speed is then the reverse speed magnitude).
    pub cmd_reverse: bool,
    /// Currently moving backwards (`speed` stays a magnitude).
    pub reversing: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Order {
    Idle,
    /// Drive a route; on arrival turn to `face` (if any) and report.
    Move {
        route: Vec<Vec2>,
        segment: usize,
        speed_limit: Option<f64>,
        face: Option<f64>,
        platoon_slot: bool,
        /// Arrival tolerance (m).
        arrive: f64,
    },
    /// Turn in place to a heading.
    Face {
        heading: f64,
    },
    /// Drive backwards, hull pointing the same way, to a point (back out of a position while
    /// keeping the gun on the threat; FM 3-20.21 (1996) ch. 2 "reverse" movement).
    Reverse {
        to: Vec2,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct VehicleAi {
    pub order: Order,
    /// What the crew reports its position as (e.g. "HULL-DOWN") once set.
    pub position_note: Option<String>,
    pub set: bool,
    /// Own (single-vehicle) move interrupted by a HALT, continued by RESUME.
    pub held: Option<Order>,
}

/// A vehicle's own picture of an enemy (crew level, before reporting).
#[derive(Debug, Clone, PartialEq)]
pub struct LocalTrack {
    /// World index of the observed entity (truth-side bookkeeping; never exported).
    pub target: usize,
    pub level: ai::picture::ContactLevel,
    pub last_seen_tick: u64,
    pub last_report_tick: u64,
    pub contact: Option<ai::picture::ContactId>,
    /// Last visible fraction and range (gunnery uses them).
    pub visible_fraction: f64,
    pub range_m: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SensorState {
    /// Sensors available at the scenario date: (kind, cycles per mrad, max range m, field of view
    /// deg; infinite for a wide view).
    pub sensors: Vec<(SensorKind, f64, f64, f64)>,
    pub tracks: Vec<LocalTrack>,
    /// Sights degraded by a hit (cycles halved).
    pub degraded: bool,
}

impl SensorState {
    pub fn track(&self, target: usize) -> Option<&LocalTrack> {
        self.tracks.iter().find(|t| t.target == target)
    }
}

/// Per-vehicle smoke equipment and the last time it was shot at.
#[derive(Debug, Clone, PartialEq)]
pub struct SmokeState {
    /// Grenade salvos left.
    pub salvos: u32,
    pub engine_on: bool,
    pub last_launch: Option<u64>,
    /// Tick a round last struck at or near the vehicle, and the bearing (compass rad) to the shooter.
    pub last_targeted: Option<(u64, f64)>,
}

/// A squad's posture (plan M8B-INFANTRY §0: three postures; occupying a building uses the
/// building's own signature and protection).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Posture {
    Moving,
    /// Kneeling and prone, at a halt or under fire.
    Halted,
    /// Halted long enough to dig in, or occupying a building.
    DugIn,
}

/// One small-arm type carried by a squad, or a vehicle's coax.
#[derive(Debug, Clone, PartialEq)]
pub struct SmallArmState {
    /// Index into the squad definition's `weapons` (unused for a coax).
    pub def: usize,
    /// Weapons of this type at full strength.
    pub count: u8,
    /// Bursts left, all weapons of the type together.
    pub bursts: u32,
    /// Bursts carried at the start.
    pub full: u32,
    /// Seconds until the next burst.
    pub cooldown: f64,
}

impl SmallArmState {
    pub fn new(def: usize, w: &SmallArmDef) -> Self {
        let bursts = w.bursts * w.count as u32;
        Self { def, count: w.count, bursts, full: bursts, cooldown: 0.0 }
    }
}

/// One anti-armour weapon type carried by a squad (plan M8B-AT-TEAMS §2.1).
#[derive(Debug, Clone, PartialEq)]
pub struct AntiArmourState {
    /// Index into the squad definition's `anti_armour`.
    pub def: usize,
    /// Launchers of this type at full strength.
    pub count: u8,
    /// Per entry of the definition's `load`: index into the sim's ammo table, and rounds left
    /// (all launchers together).
    pub ammo: Vec<usize>,
    pub stock: Vec<u32>,
    /// Seconds until the next shot of this type.
    pub cooldown: f64,
}

impl AntiArmourState {
    pub fn rounds(&self) -> u32 {
        self.stock.iter().sum()
    }
}

/// A dismounted squad: one abstract element (plan M8B-INFANTRY §1–§2, DESIGN.md §14 #29).
#[derive(Debug, Clone, PartialEq)]
pub struct SquadState {
    /// Index into the sim's squad definition table.
    pub def: usize,
    /// Men at full strength.
    pub men: u8,
    /// Men fit to fight.
    pub strength: u8,
    pub posture: Posture,
    /// Seconds halted in place (digging in).
    pub halted_s: f64,
    /// Carrier (world index) it rides in; while mounted it has no position of its own.
    pub mounted_in: Option<usize>,
    /// Carrier it belongs to (where MOUNT sends it).
    pub carrier: Option<usize>,
    /// Building (`BakedBuilding::id`) it occupies.
    pub in_building: Option<u32>,
    pub weapons: Vec<SmallArmState>,
    /// Seconds left of the mount drill once at the carrier.
    pub boarding: Option<f64>,
    /// Current foot speed (m/s).
    pub speed: f64,
    /// Target of its small arms (world index).
    pub target: Option<usize>,
    /// Below the doctrine's strength threshold: defends in place, counts as lost.
    pub ineffective: bool,
    /// Body armour worn (from the definition by year): multipliers on the chance a bullet hit
    /// takes a man out and on HE fragment exposure (1 = none).
    pub bullet_factor: f64,
    pub fragment_factor: f64,
    /// Night aiming error multiplier of its aiming aid, if it carries one.
    pub night_aid: Option<f64>,
    /// Rockets and missiles (plan M8B-AT-TEAMS §2).
    pub anti_armour: Vec<AntiArmourState>,
    /// Ambush sprung: the squad has fired its anti-armour weapons or been fired on, so it no
    /// longer holds fire for the kill zone (plan M8B-AT-TEAMS §2.2).
    pub sprung: bool,
}

/// An IFV's troop compartment.
#[derive(Debug, Clone, PartialEq)]
pub struct CarrierState {
    pub capacity_men: u8,
    /// Squads aboard (world indices), in boarding order.
    pub embarked: Vec<usize>,
    /// Ramp drill in progress: seconds left before the squads are out.
    pub dismounting: Option<f64>,
}

#[derive(Debug, Default, Clone)]
pub struct World {
    next_id: u32,
    pub id: Vec<EntityId>,
    pub side: Vec<SideId>,
    pub callsign: Vec<String>,
    pub transform: Vec<Transform>,
    /// Index into the sim's vehicle definition table.
    pub vehicle: Vec<Option<usize>>,
    pub mobility: Vec<Option<MobilityState>>,
    pub ai: Vec<Option<VehicleAi>>,
    pub sensors: Vec<Option<SensorState>>,
    pub gun: Vec<Option<crate::gunnery::GunState>>,
    pub condition: Vec<Condition>,
    pub weapons: Vec<WeaponsStatus>,
    pub platoon: Vec<Option<PlatoonId>>,
    /// Tick of the last main-gun shot (detection cue).
    pub last_fired: Vec<Option<u64>>,
    /// Tick and signature factor of the last rocket or missile launch (backblast: a stronger
    /// firing cue; plan M8B-AT-TEAMS §2.2).
    pub launch: Vec<Option<(u64, f64)>>,
    pub smoke: Vec<SmokeState>,
    /// Reactive armour tiles left.
    pub era: Vec<u32>,
    /// Crew suppression, 0..1: rises under fire, decays (fires.ron).
    pub suppression: Vec<f64>,
    /// Dismounted infantry (plan M8B-INFANTRY §2).
    pub squad: Vec<Option<SquadState>>,
    pub carrier: Vec<Option<CarrierState>>,
    /// Coaxial machine gun.
    pub coax: Vec<Option<SmallArmState>>,
}

impl World {
    pub fn len(&self) -> usize {
        self.id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.id.is_empty()
    }

    pub fn index_of(&self, id: EntityId) -> Option<usize> {
        // Ids are allocated in increasing order and never removed, so the vector is sorted.
        self.id.binary_search(&id).ok()
    }

    pub fn alive(&self, k: usize) -> bool {
        self.condition[k] != Condition::Destroyed
    }

    pub fn can_move(&self, k: usize) -> bool {
        matches!(self.condition[k], Condition::Operational | Condition::FirepowerKill)
    }

    /// A squad riding in a carrier: not on the ground, not seen, not shot at.
    pub fn mounted(&self, k: usize) -> bool {
        self.squad[k].as_ref().is_some_and(|s| s.mounted_in.is_some())
    }

    /// Still a fighting element: vehicles with a gun or able to move, squads above their
    /// combat-effective threshold.
    pub fn fighting(&self, k: usize) -> bool {
        match &self.squad[k] {
            Some(s) => self.alive(k) && !s.ineffective,
            None => matches!(self.condition[k], Condition::Operational | Condition::MobilityKill),
        }
    }

    fn push_common(&mut self, side: SideId, callsign: &str, pos: Vec2, z: f64, heading: f64) -> EntityId {
        self.next_id += 1;
        let id = EntityId(self.next_id);
        self.id.push(id);
        self.side.push(side);
        self.callsign.push(callsign.to_owned());
        self.transform.push(Transform { pos, z, heading, pitch: 0.0, roll: 0.0 });
        self.condition.push(Condition::Operational);
        self.weapons.push(WeaponsStatus::Free);
        self.platoon.push(None);
        self.last_fired.push(None);
        self.launch.push(None);
        self.suppression.push(0.0);
        self.smoke.push(SmokeState { salvos: 0, engine_on: false, last_launch: None, last_targeted: None });
        self.era.push(0);
        id
    }

    /// Spawn a squad, on the ground or (with `carrier`) mounted in it.
    #[allow(clippy::too_many_arguments)]
    pub fn spawn_squad(
        &mut self,
        side: SideId,
        callsign: &str,
        squad_index: usize,
        def: &SquadDef,
        pos: Vec2,
        z: f64,
        heading: f64,
        year: i32,
        carrier: Option<usize>,
        anti_armour: Vec<AntiArmourState>,
    ) -> EntityId {
        let id = self.push_common(side, callsign, pos, z, heading);
        self.vehicle.push(None);
        self.mobility.push(None);
        self.ai.push(Some(VehicleAi { order: Order::Idle, position_note: None, set: false, held: None }));
        let sensors = def
            .sensors
            .iter()
            .filter(|s| s.in_service.contains(year))
            .map(|s| {
                let max = s.max_range_m.as_ref().map_or(f64::INFINITY, |r| r.get());
                (s.kind, s.cycles_per_mrad.get(), max, s.field_of_view_deg.as_ref().map_or(f64::INFINITY, |f| f.get()))
            })
            .collect();
        self.sensors.push(Some(SensorState { sensors, tracks: Vec::new(), degraded: false }));
        self.gun.push(None);
        let weapons = def
            .weapons
            .iter()
            .enumerate()
            .filter(|(_, w)| w.in_service.is_none_or(|d| d.contains(year)))
            .map(|(i, w)| SmallArmState::new(i, w))
            .collect();
        self.squad.push(Some(SquadState {
            def: squad_index,
            men: def.men,
            strength: def.men,
            posture: Posture::Halted,
            halted_s: 0.0,
            mounted_in: carrier,
            carrier,
            in_building: None,
            weapons,
            boarding: None,
            speed: 0.0,
            target: None,
            ineffective: false,
            bullet_factor: def.armour_in(year).map_or(1.0, |a| a.bullet_factor.get()),
            fragment_factor: def.armour_in(year).map_or(1.0, |a| a.fragment_factor.get()),
            night_aid: def.aiming_aid_in(year).map(|a| a.night_factor.get()),
            anti_armour,
            sprung: false,
        }));
        self.carrier.push(None);
        self.coax.push(None);
        let k = self.len() - 1;
        if let Some(c) = carrier
            && let Some(cs) = self.carrier[c].as_mut()
        {
            cs.embarked.push(k);
        }
        id
    }

    #[allow(clippy::too_many_arguments)]
    pub fn spawn_vehicle(
        &mut self,
        side: SideId,
        callsign: &str,
        vehicle_index: usize,
        def: &VehicleDef,
        pos: Vec2,
        z: f64,
        heading: f64,
        year: i32,
        stock: Vec<u32>,
    ) -> EntityId {
        let id = self.push_common(side, callsign, pos, z, heading);
        self.vehicle.push(Some(vehicle_index));
        self.mobility.push(Some(MobilityState {
            speed: 0.0,
            fuel_l: def.mobility.fuel_capacity_l.get(),
            stalled_s: 0.0,
            cmd_speed: 0.0,
            cmd_heading: heading,
            cmd_reverse: false,
            reversing: false,
        }));
        self.ai.push(Some(VehicleAi { order: Order::Idle, position_note: None, set: false, held: None }));
        let sensors = def
            .sensors
            .iter()
            .filter(|s| s.in_service.contains(year))
            .map(|s| {
                let max = s.max_range_m.as_ref().map_or(f64::INFINITY, |r| r.get());
                (s.kind, s.cycles_per_mrad.get(), max, s.field_of_view_deg.as_ref().map_or(f64::INFINITY, |f| f.get()))
            })
            .collect();
        self.sensors.push(Some(SensorState { sensors, tracks: Vec::new(), degraded: false }));
        self.gun.push(Some(crate::gunnery::GunState::new(heading, stock)));
        let k = self.len() - 1;
        self.era[k] = def.armour.era.as_ref().filter(|e| e.in_service.contains(year)).map_or(0, |e| e.tiles);
        self.smoke[k].salvos = def.smoke_launcher.as_ref().map_or(0, |l| l.salvos);
        self.squad.push(None);
        self.carrier.push(def.troops.as_ref().map(|t| CarrierState {
            capacity_men: t.capacity_men,
            embarked: Vec::new(),
            dismounting: None,
        }));
        self.coax.push(def.coax.as_ref().map(|c| SmallArmState::new(0, c)));
        id
    }

    #[allow(clippy::too_many_arguments)]
    pub fn set_route(
        &mut self,
        k: usize,
        route: Vec<Vec2>,
        speed_limit: Option<f64>,
        face: Option<f64>,
        slot: bool,
        arrive: f64,
    ) {
        if let Some(ai) = &mut self.ai[k] {
            ai.order = Order::Move { route, segment: 0, speed_limit, face, platoon_slot: slot, arrive };
            ai.set = false;
            ai.held = None;
            ai.position_note = None;
        }
        if let Some(m) = &mut self.mobility[k] {
            m.stalled_s = 0.0;
        }
    }

    /// Stop in place. A vehicle's own move is kept for `resume`; platoon slot routes are not
    /// (the platoon keeps its order instead).
    pub fn halt(&mut self, k: usize) {
        if let Some(ai) = &mut self.ai[k] {
            let o = std::mem::replace(&mut ai.order, Order::Idle);
            if matches!(o, Order::Move { platoon_slot: false, .. }) {
                ai.held = Some(o);
            }
        }
        if let Some(m) = &mut self.mobility[k] {
            m.cmd_speed = 0.0;
        }
    }

    /// Change the speed limit of a vehicle's own move (current and held).
    pub fn set_speed_limit(&mut self, k: usize, limit: Option<f64>) {
        let Some(ai) = &mut self.ai[k] else { return };
        for o in [Some(&mut ai.order), ai.held.as_mut()].into_iter().flatten() {
            if let Order::Move { speed_limit, platoon_slot: false, .. } = o {
                *speed_limit = limit;
            }
        }
    }

    /// Continue a held move. Returns false if nothing was held.
    pub fn resume(&mut self, k: usize) -> bool {
        let Some(ai) = &mut self.ai[k] else { return false };
        let Some(o) = ai.held.take() else { return false };
        ai.order = o;
        ai.set = false;
        ai.position_note = None;
        if let Some(m) = &mut self.mobility[k] {
            m.stalled_s = 0.0;
        }
        true
    }

    pub(crate) fn hash_into(&self, h: &mut StableHasher) {
        for k in 0..self.len() {
            h.u32(self.id[k].0);
            h.u8(self.side[k].0);
            h.u8(self.condition[k] as u8);
            let t = &self.transform[k];
            for v in [t.pos[0], t.pos[1], t.z, t.heading, t.pitch, t.roll] {
                h.f64(v);
            }
            if let Some(m) = &self.mobility[k] {
                for v in [m.speed, m.fuel_l, m.stalled_s, m.cmd_speed, m.cmd_heading] {
                    h.f64(v);
                }
                if m.reversing || m.cmd_reverse {
                    h.u8(1 + m.reversing as u8);
                }
            }
            if let Some(s) = &self.sensors[k] {
                h.u64(s.tracks.len() as u64);
                for t in &s.tracks {
                    h.u64(t.target as u64);
                    h.u8(t.level as u8);
                }
            }
            if let Some(g) = &self.gun[k] {
                g.hash_into(h);
            }
            let sm = &self.smoke[k];
            h.u32(sm.salvos);
            h.u32(self.era[k]);
            h.f64(self.suppression[k]);
            h.u8(sm.engine_on as u8);
            h.u64(sm.last_launch.unwrap_or(u64::MAX));
            if let Some(s) = &self.squad[k] {
                h.u8(s.strength);
                h.u8(s.posture as u8);
                h.f64(s.halted_s);
                h.u64(s.mounted_in.map_or(u64::MAX, |c| c as u64));
                h.u32(s.in_building.unwrap_or(0));
                h.f64(s.boarding.unwrap_or(-1.0));
                h.f64(s.speed);
                h.u64(s.target.map_or(u64::MAX, |t| t as u64));
                h.u8(s.ineffective as u8);
                for w in &s.weapons {
                    h.u32(w.bursts);
                    h.f64(w.cooldown);
                }
                for w in &s.anti_armour {
                    for &n in &w.stock {
                        h.u32(n);
                    }
                    h.f64(w.cooldown);
                }
                h.u8(s.sprung as u8);
            }
            if let Some(c) = &self.carrier[k]
                && (!c.embarked.is_empty() || c.dismounting.is_some())
            {
                h.u64(c.embarked.len() as u64);
                h.f64(c.dismounting.unwrap_or(-1.0));
            }
            // Only once used, so scenarios without infantry keep their hashes.
            if let Some(c) = &self.coax[k]
                && (c.cooldown > 0.0 || c.bursts != c.full)
            {
                h.u32(c.bursts);
                h.f64(c.cooldown);
            }
            if let Some(ai) = &self.ai[k] {
                match &ai.order {
                    Order::Idle => h.u8(0),
                    Order::Move { route, segment, .. } => {
                        h.u8(1);
                        h.u64(*segment as u64);
                        h.u64(route.len() as u64);
                    }
                    Order::Face { heading } => {
                        h.u8(2);
                        h.f64(*heading);
                    }
                    Order::Reverse { to } => {
                        h.u8(3);
                        h.f64(to[0]);
                        h.f64(to[1]);
                    }
                }
            }
        }
    }
}
