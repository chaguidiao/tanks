# M8c slice 7: recovery of immobilised vehicles

Status: **2026-11-04, defaults taken, built for review** (TODO C2, plan `M8C-LOGISTICS.md` §2 "Recovery", §5 step 7). The user was away; the recommendations in §8 were built as written so the slice can be reviewed working. Any of them can be changed before merge.

Goal: a mobility-killed vehicle is not lost for good. A recovery vehicle drives to it, hooks up, tows it to the collection point and, after a repair time from data, it is operational again. Recovery costs time and exposes the recovery vehicle; losing it loses the side's ability to recover.

## 1. Doctrine

FM 17-15 (1996), on tank platoon combat service support, and FM 71-1 (1998), on the company team: crews first try to recover their own vehicle; what they cannot fix or move is reported, and the battalion maintenance section recovers it with a recovery vehicle to the unit maintenance collection point (UMCP), normally in or near the combat trains, where it is repaired or sent further back. FM 20-22 (1988) *Vehicle Recovery Operations* and its successors (FM 4-30.31 (2006), ATP 4-31) describe the methods (self-, like-vehicle and recovery-vehicle recovery), towing with a tow bar, and the principle of repairing as far forward as possible. None give per-minute times; timings are `estimate`.

## 2. What exists

- `Condition::MobilityKill` is permanent; such a vehicle still fights (gun) and counts as a fighting vehicle for victory.
- Slice 6 added `VehicleClass::Support`, the "not a fighting vehicle" rule for trucks (`World::fighting`, victory force), trucks as the last target (`ContactKind::Supply`), and the platoon-order pattern for logistics work (`PlatoonOrder::Deliver`, a served platoon and a working platoon linked by id).

## 3. Model

**Recovery vehicle types (data).** New `VehicleClass::Recovery` (its own `TargetClass` so artillery treats it as the armoured vehicle it is, not as a truck) and an optional `recovery` block on `VehicleDef`: `tow_speed_mps` (towing a vehicle of up to the rated mass), `rated_tow_kg` (heavier vehicles are towed slower, in proportion), `hookup_s` and `unhook_s`. Unarmed in the model (`load: []`, no coax: the pintle MG is for self-defence and is left out like the trucks' weapons). Initial types:
- Blue HAWSER A1 mirroring the M88A1 (1977–), on an M48/M60-class hull. The M88A1 was rated for the M60-class tanks; towing the heavier M1 family was its known limit, which the rated mass carries (`low`/`estimate`).
- Red BUKSIR mirroring the BREM-1 (1984–), on a T-72 hull.

**Collection point.** The recovery vehicle's own starting position (where the scenario puts it: the combat trains). No new scenario item in this slice (§8 #2).

**Order.** `Command::Recover { platoon, by }`: the recovery vehicles of platoon `by` recover the immobilised vehicles of `platoon`. The damaged platoon keeps its own order (it fights on; the immobilised vehicle is simply left behind by it). Each recovery vehicle in turn:
1. takes the next immobilised, live member in platoon order that no other recovery vehicle has taken;
2. drives to a spot `hitch_m` (data, 10 m) behind it (the side away from where its hull faces, so normally away from the enemy);
3. halts for `hookup_s`, then tows it to the collection point at the tow speed: the towed vehicle follows `hitch_m` behind on the tow bar, every tick;
4. at the collection point halts for `unhook_s`, leaves it and the vehicle's repair starts: after `repair_mobility_s` (data in `logistics.ron`) it is `Operational` again and reports `REPAIRED`;
5. takes the next vehicle, or reports `RECOVERY COMPLETE` and holds at the collection point when none is left.
A recovery vehicle that is destroyed or immobilised drops its tow where it is (the towed vehicle stays immobilised, free to be taken again). A towed vehicle that is destroyed ends that tow. A new order to the recovery platoon ends its work (a vehicle on the tow bar is dropped where it is). Vehicles newly immobilised while the order runs are taken too.

**Repaired vehicles** rejoin their platoon at its next order (they drive with it like any member). They are repaired with the fuel and rounds they had.

**Victory.** Recovery vehicles are not fighting vehicles (same rule as trucks, §8 #3 of the tailgate plan). A vehicle under tow or in repair is a mobility kill until repaired, so it still counts as before.

## 4. Picture and client

- `OwnUnit.recovery: Option<RecoveryStage>`: on a damaged vehicle `Awaiting`, `UnderTow`, `InRepair { ready_in_s }`; on a recovery vehicle `ToVehicle`, `HookingUp`, `Towing`, `Unhooking`, `Idle`.
- Enemy recovery vehicles are recognised as `ContactKind::Supply` (unarmed logistics vehicle: last target, like trucks).
- Card: **RECOVER** in the FIRES / SUPPORT / STAFF row of a platoon with an immobilised vehicle (with one recovery platoon it uses that one; with several, a popup like RESUPPLY's sources). Status card lines (stage, repair time left). Radio: `R: MOVING TO 1-2.`, `R: 1-2 HOOKED UP, TOWING TO UMCP.`, `1-2: REPAIRED, AT UMCP.`, `R: RECOVERY COMPLETE.` APP-6D maintenance (recovery) symbol on own and identified enemy recovery vehicles.
- DATABASE: recovery rows (tow speed, rated mass, hookup and unhook times) and the repair time.

## 5. AI and drills

- No react-to-fire withdrawal in this slice: the recovery vehicle is armoured and doctrine sends it forward under the platoon's cover (FM 20-22: recovery under fire is done with covering fire and smoke where possible). If testing shows it drives into obvious kill zones, the tailgate trucks' WITHDRAW SOP is the model (deferred, TODO).
- Commander AI sending recovery vehicles: slice 8 with the resupply threshold.

## 6. Code changes (order of commits)

1. Data: `VehicleClass::Recovery`, `TargetClass::Recovery`, `VehicleDef.recovery`, the two vehicle files, `fires.ron` HE row for `recovery`, `logistics.ron` `hitch_m`, `repair_mobility_s`; loader rules (a recovery vehicle has a `recovery` block and an empty load, others none); DATABASE rows.
2. Sim: `World.tow` component (Some marks a recovery vehicle; holds the vehicle on the tow bar), `World::support` (trucks and recovery vehicles), `Command::Recover`, `PlatoonOrder::Recover` with per-vehicle `RecoveryJob`s, tow placement after mobility, `Sim.repairs`, reports, picture stages, hash. Tests (`m8c.rs`): a mobility kill is recovered, towed home, repaired and operational after the data time; a heavier vehicle is towed slower; two immobilised vehicles are taken in turn; a recovery vehicle killed on the way drops the tow; recovery vehicles never fire and do not count for victory; replay determinism.
3. Client: RECOVER button and popup, status lines, radio, symbol.

## 7. Determinism, hash, balance

No randomness; who is taken first is platoon slot order, recovery vehicles in their platoon order. Tow and repair state hashed only when present, so the golden hash and every existing scenario (none has a recovery vehicle) are unchanged; balance batch not affected. `SIM_VERSION` bump (new command).

## 8. Decisions (defaults built; review)

| # | Question | Options | Default built |
|---|---|---|---|
| 1 | New class or `Support` with a block? | (a) `VehicleClass::Recovery`; (b) a `Support` vehicle with a `recovery` block | **(a)**: artillery effects are by class and an M88 is not a soft-skinned truck |
| 2 | Collection point | (a) the recovery vehicle's start position; (b) a new scenario item (UMCP) with editor placement; (c) the nearest own supply point | **(a)** now; (b) when the exit scenario needs it (TODO C6) |
| 3 | Order addressing | (a) `Recover { platoon, by }`: all immobilised vehicles of a platoon; (b) one vehicle at a time | **(a)**, like `Tailgate`; a platoon card is where the player sees the damage |
| 4 | Repair | (a) every mobility kill repairable after a fixed data time; (b) a share beyond unit repair (random, evacuated) | **(a)**: deterministic and simple; (b) is a later fidelity item |
| 5 | Firepower kills | (a) out of scope (the vehicle can drive back itself); (b) a REPAIR order at the collection point | **(a)**, TODO C6 |
| 6 | Under fire | (a) carries on; (b) withdraws like the trucks | **(a)**, armoured; see §5 |
