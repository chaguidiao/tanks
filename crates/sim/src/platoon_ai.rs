//! Platoon AI (1 Hz + event-driven). Filled in by M4.

use terrain::BakedMap;
use terrain::path::Pathfinder;

use crate::Event;
use crate::platoon::Platoon;
use crate::world::World;

pub struct Ctx<'a> {
    pub world: &'a mut World,
    pub vehicles: &'a [sim_data::VehicleDef],
    pub map: &'a BakedMap,
    pub td: &'a sim_data::TerrainData,
    pub pictures: &'a [ai::picture::PerceivedWorld],
    pub doctrines: &'a [sim_data::DoctrinePack],
    pub pathfinder: &'a mut Pathfinder,
    pub tick: u64,
    pub events: &'a mut Vec<Event>,
}

pub fn step(_platoons: &mut [Platoon], _ctx: &mut Ctx) {}
