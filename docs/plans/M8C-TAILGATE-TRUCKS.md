# M8c slice 6: tailgate resupply by supply trucks

Status: **2026-10-31, agreed** (TODO C1, plan `M8C-LOGISTICS.md` §2 "Tailgate method", §5 step 6). All six recommendations in §8 accepted by the user on 2026-10-31.

Goal: resupply without leaving the position. Supply trucks are vehicles that drive to a platoon, go from vehicle to vehicle and hand over fuel and rounds, so resupply costs exposure instead of the platoon's absence. Trucks can be seen and killed, and losing them is losing the stock they carry.

## 1. Doctrine

FM 17-15 (1996) and ATP 3-20.15 (2025), on tank platoon sustainment, give two methods for a LOGPAC: **service station** (vehicles leave their positions a few at a time and drive to the resupply point: built in slices 1–4) and **tailgate** (the trucks drive to each vehicle in its position, or just behind it, used when the platoon must stay in place and the threat allows trucks forward). Tailgate is slower for the unit as a whole (the trucks visit vehicles one by one) but keeps every gun in position; it exposes the trucks. FM 71-1 (1998) and ATP 3-90.1 put the company's LOGPAC under the first sergeant: trucks come forward from the trains to a logistics release point and go back when done.

## 2. What exists and what it assumes

- `logistics::transfer(world, k, &mut SupplyPoint, ...)` moves fuel, rounds and smoke salvos at the data rates (`logistics.ron`), in whole rounds, conserving stock; `ServiceState` tracks one member. This is reused unchanged for trucks.
- Every `VehicleDef` has a main armament (`ArmamentDef` is not optional, 28 uses, 14 in `gunnery.rs`), a class (`Mbt | Ifv | Recon`), track-based mobility, and counts as fighting for victory while operational (`World::fighting`). HE effects are looked up by class (`fires.ron`; a missing class means artillery has **no effect** on it).

## 3. Model

**Truck types (data).** New `VehicleClass::Support` and an optional `cargo` block on `VehicleDef`: capacity in fuel litres, rounds per ammo id, smoke salvos. Initial types (fictional names, public sources, in-service dates):
- Blue cargo truck mirroring the M977 HEMTT (1982–), ammunition; Blue fuel truck mirroring the M978 HEMTT tanker (1982–, 9,500 l).
- Red cargo truck mirroring the Ural-4320 (1977–); Red fuel truck mirroring the ATZ-5-4320 (5,000 l).
Payload in rounds is derived from payload mass and packed-round mass (`estimate` with the arithmetic in `notes`). Armour: thin steel only (cab and body a few mm, `estimate`). Armament: an empty load (`load: []`, no gun), so gunnery skips it (§6 checks every path); a vehicle with no rounds never selects a target.
Mobility: the track-based model with an equivalent contact patch for six or eight wheels and a low cross-country speed (`estimate`); a proper wheeled model (road-bound, soft-ground bogging) goes to TODO as an ENG item.

**Stock on the truck.** Each truck carries a `Stock` (the fuel/rounds/salvos fields split out of `SupplyPoint`; a supply point then owns a `Stock` too). Starting cargo = the type's full capacity unless the scenario unit gives `cargo` (override, like battery rounds). Trucks' cargo is hashed.

**Organisation.** Trucks are a platoon of the scenario like any other (callsign e.g. `SUP`), of `Support` vehicles only. They fight nothing: no contact drill except **react to fire** (§5).

**Order.** `Command::Resupply { platoon, source }` gains `source: Source::Point(id) | Source::Trucks(platoon)` (the old form deserialises as `Point`, so replays load). Tailgate:
1. The trucks drive to a **release point** behind the platoon: the platoon centroid moved `release_m` (data, 300 m) away from the platoon's threat direction (its BP facing, else its heading), routed like a move.
2. Each truck takes the members in platoon-slot order that still need something it carries and that no other truck is serving; it parks `tailgate_m` (data, 12 m) behind the vehicle (rear side, relative to the vehicle's hull) and transfers with `logistics::transfer` at the existing rates. While a vehicle is served it holds (no move orders) and does not fire its main gun while main-gun rounds are being handed over (crew busy), as the service station already does.
3. A truck moves on when the member is full in what it carries or it is empty; a member is done when no live truck has anything it needs.
4. When every member is done, the trucks drive back to where they came from and the platoon reports `RESUPPLY COMPLETE` / `PARTIAL. TRUCKS EMPTY.`; the truck platoon reports `SUP: RETURNING, n RDS, m L LEFT.`
A new order to the served platoon ends its part; a new order to the trucks ends theirs (partial transfers are kept). A truck that is killed loses its cargo.

**Refill at a supply point (stretch, §8 #4).** `Command::Refill { trucks, point }`: the trucks drive to a supply point and load from it at the same rates (point → truck, conserved). This links slice 1's points with the trucks into a chain (point in the rear, trucks forward).

## 4. Picture and client

- Own trucks are `OwnUnit`s with a `cargo` summary; enemy trucks are contacts like any vehicle (APP-6D supply symbol for a recognised `Support` class).
- Command card RESUPPLY (R): with both a supply point and a truck platoon available, the value cell cycles **AT SP 1 / BY TRUCKS SUP** (or a small popup like the ammunition list); with one, it uses that one. The truck platoon's card shows its cargo and a REFILL button if §8 #4 is taken.
- Platoon card: "RESUPPLY BY SUP: 2 SERVED, 1 BEING SERVED, 1 WAITING"; truck card "SERVING 2-3".
- Scenario editor: trucks are units like any other (vehicle list filtered by date); the unit row shows the cargo, `[ ]` halves/doubles it like a supply point.

## 5. AI and drills

- **Truck react to fire** (convoy drill "react to direct fire": drive out of the kill zone; source to confirm in REFERENCES.md before coding): a truck that is hit, near-missed or within 150 m of a burst stops serving and drives back to the release point; the trucks report `SUP: UNDER FIRE, WITHDRAWING.` Resumes on a new order. Default SOP, overridable as DESIGN.md §14 #34 asks of new drills (a `tailgate_under_fire` setting: WITHDRAW (default) / CONTINUE).
- **Enemy target priority:** trucks are valid targets but the lowest priority class; HE preferred (`preferred_vs` gets `support`).
- **Commander AI** sending trucks: slice 8, not here.

## 6. Code changes (order of commits)

1. Data: `VehicleClass::Support`, `TargetClass::Support`, `VehicleDef.cargo`, four truck files, `fires.ron` HE effect row and `damage.ron` rows for `support` (soft-skinned: most hits catastrophic, `estimate`), `logistics.ron` `release_m`, `tailgate_m`; DATABASE sheet (cargo rows, no armament section for an empty load). Loader tests.
2. Sim, structure: `Stock` split out of `SupplyPoint`; trucks spawn with a `cargo: Option<Stock>` component; gunnery/vehicle AI paths audited for an empty load (tests: a truck never fires, never panics, is engaged and killed by HE and coax); victory: `Support` vehicles are not fighting vehicles (do not hold an objective, do not count as lost force for "force destroyed"), decision §8 #3.
3. Sim, tailgate: `Source`, `PlatoonOrder::Tailgate` (served platoon) and `PlatoonOrder::Deliver` (trucks), assignment and transfer, reports, react to fire. Tests (`m8c.rs`): stock conserved truck → vehicles; two trucks (fuel and cargo) serve one vehicle each in turn; a platoon in a BP stays in it and keeps firing with the members not being served; a killed truck's cargo is gone and the report says PARTIAL; replay determinism; a replay recorded before the change still loads.
4. Client: card source choice, truck cards, symbol, radio, editor rows.
5. Stretch: refill at a supply point.

## 7. Determinism, hash, balance

Assignment order is platoon slot then entity id; no randomness. Truck cargo and tailgate state hashed. Golden hash unchanged (no trucks in it); the v1 scenarios have no trucks, so the balance batch should be unchanged run for run (it is re-run to prove it, because victory and target-priority code change). `SIM_VERSION` bump (new command field).

## 8. Decisions

| # | Question | Options | Recommendation |
|---|---|---|---|
| 1 | Truck types: separate fuel and cargo trucks, or one combined type per side? | (a) separate, as the real LOGPAC; (b) one combined truck | **(a)**: two trucks with different cargo make the "who serves what" logic real at little cost, and losing the fuel truck is a different problem from losing the ammunition |
| 2 | Where does the truck stand while serving? | (a) `tailgate_m` behind the vehicle's hull; (b) at a release point, vehicles back up to it one at a time | **(a)**: that is the tailgate method; (b) is the service station again |
| 3 | Trucks and victory | (a) not fighting vehicles: never hold objectives, not counted in "force destroyed"; (b) count like any vehicle | **(a)**: otherwise a side keeps "holding" with trucks and loses its force ratio to soft kills |
| 4 | Refill trucks at a supply point in this slice? | (a) yes (point → truck chain); (b) later | **(b) later**, unless you want the chain in the M8c exit scenario: it is a self-contained addition |
| 5 | Mobility of wheeled trucks | (a) track model with an equivalent contact patch (estimate) now, proper wheeled model later (TODO ENG); (b) wheeled model first | **(a)**: the trucks mostly drive on roads and short tracks behind positions; the gap is noted |
| 6 | Served vehicle and fire | (a) holds and does not fire the main gun while main-gun rounds are loaded, other members fight; (b) no restriction | **(a)**, matching the service station and the crew task |
