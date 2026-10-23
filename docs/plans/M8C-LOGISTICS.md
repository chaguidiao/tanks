# M8c logistics: plan

Status: **2026-10-21, working plan** (TODO.md M8c: C1–C4). Not gated by the infantry decisions (`M8B-INFANTRY.md`): supply trucks and recovery vehicles are vehicles. The defaults below are what gets built unless the user changes them.

Goal: ammunition and fuel run out, can be replaced, and the replacement costs time and exposure; immobilised vehicles can be recovered; batteries run dry and resupply. Exit (TODO): a long scenario where ammunition decides the outcome, and stock accounting tests.

## 1. What exists

- Per-vehicle stock per loadout entry (`GunState.stock`, hashed); smoke salvos (`SmokeState.salvos`); fuel (`MobilityState.fuel_l`, burned by power; a vehicle out of fuel stops). Nothing refills any of them.
- Batteries are scenario assets with HE/smoke round counts (`BatteryAssign`, DESIGN.md §14 #21); empty means no more missions.
- `Condition::MobilityKill` is permanent; such a vehicle still counts as a fighting vehicle for victory (`victory.rs`).

## 2. Model

**Resupply is a transfer at a rate, not an instant refill.** Each transfer is a sim activity that takes time from data, during which the vehicle is halted and (for main-gun ammunition) its turret is not engaging. Sources: FM 17-15 (1996) and ATP 3-20.15 (2025) on combat service support of the tank platoon (LOGPAC, service-station and tailgate resupply), FM 71-1 (1998) and ATP 3-90.1 on company-team sustainment. Rates we cannot source are `estimate` with reasoning (e.g. rounds per minute a crew can stow by hand; litres per minute of a tanker's pump).

- **Supply point (C1, first slice):** a scenario item per side: position, radius, stock (fuel litres, rounds per ammo id, smoke salvos; a missing entry = none, `unlimited` allowed for training). Shown as a control-measure graphic (APP-6D supply point). Not an entity: it cannot be shot in v1.x (trucks can, slice 3).
- **Service-station method:** `Command::Resupply { platoon, point }`. The platoon moves to the point as a normal route, then vehicles are served at the point a few at a time (`stations` from the point's data); others wait in a holding position near it. When all are full or the point runs dry the platoon reports `RESUPPLY COMPLETE` (or `PARTIAL`) and holds. A new order interrupts it; partial transfers are kept.
- **Tailgate method (slice 3):** supply trucks are vehicles (`VehicleDef.class = Support`, `cargo` block); `Command::Resupply { platoon, truck }` sends the trucks to the platoon, which stays in its positions. Trucks are entities, so they can be seen and killed.
- **Re-arm events (C4):** a scenario `logistics` list with `(at_s, side, platoons, kind)` where kind is `Rearm { fraction }` / `Refuel { fraction }` / `Battery { callsign, he, smoke }`: an off-map LOGPAC that refills at a time, applied as a generated command (DESIGN.md §14 #14: replays regenerate it). **Built (slice 3):** `ScenarioDef.logistics: Vec<LogisticsEventDef>` (omitted when empty); `platoons` empty = every platoon of the side. `Rearm` adds `fraction` of the full load of every round type and of the smoke salvos (whole rounds, rounded down), `Refuel` adds `fraction` of the fuel capacity, both capped at the full load; `Battery` adds rounds to the side's battery (no cap: a battery's stock has no fixed maximum yet, C3). Instant, no transfer time (the LOGPAC is off the map). Callsigns resolve at load (unknown platoon or battery, bad time or fraction = load error) to `Command::Logpac`, applied at `round(at_s / dt)` as a generated command; the sim ignores a `Logpac` given from outside. Reports: `LOGPAC RECEIVED. REARMED.` / `REFUELLED.` per platoon with a live vehicle, `RESUPPLIED. n HE, m SMOKE.` from the battery. No new hashed state (the events follow from the scenario and the tick).
- **Battery resupply and relocation (C3):** per battery type `resupply_s` (time to restock after it runs dry or drops below a threshold, from the trains) and optional relocation: after `displace_after` missions the battery is unavailable for `displace_s` (survivability move). Data per battery, `estimate` where needed.
- **Recovery (C2):** a recovery vehicle (`class = Recovery`, `recovery { tow_speed_mps, hookup_s }`) ordered to a mobility-killed vehicle drives to it, hooks up, tows it to a collection point; after `repair_s` (data per damage kind) at the point it is `Operational` again. Self-recovery and towing by another tank are out of scope for v1.x.

All timings, rates and stocks live in `data/combat/logistics.ron` (and the vehicle/battery files for per-type values), each with `source`, `confidence` and `notes`.

## 3. Determinism, hash, replays

- Transfers advance in whole ticks at fixed rates; who is served first is ordered by platoon slot, then entity id. Stocks, transfer state and supply-point stocks are hashed. No randomness in slice 1.
- Golden hash: unchanged until a scenario uses logistics (no supply points, no events = no behaviour change); the commit that adds state to the hash says so.
- New `Command` variants are additive; replays without them load unchanged.

## 4. Picture and UI

- `PerceivedWorld` gains the side's own supply points and stocks, own trucks/recovery vehicles as units, and per-vehicle ammo per type (rounds left exists already in `UnitView`), a `ServiceState` on own units (moving to point, waiting, being served, done).
- Client: supply-point symbol, RESUPPLY on the command card (hotkey, tooltip with the method and the stock at the point), status-card line "RESUPPLY 12/40 RDS", radio `2: AT SUPPLY POINT, RESUPPLY COMPLETE.`, roster ammo/fuel warnings, scenario editor placement of supply points and events (with J5).
- AI: commander AI sends platoons below an ammunition threshold to resupply when not in contact (doctrine pack `logistics { ammo_threshold, fuel_threshold }`), so the OPFOR and the auto-commander use it too.

## 5. Order of work (each about one commit)

1. Data: `logistics.ron` (rates, station count default), supply-point and event types in the scenario format, loader tests, database "LOGISTICS" rows with sources.
2. Sim: supply points, `Command::Resupply` service station, transfer, reports, hash; unit tests for stock accounting (rounds conserved between point and vehicles, fuel capped by capacity, partial transfer kept on interruption).
3. C4 re-arm events.
4. Client: symbol, card button, status, radio, editor placement.
5. C3 battery resupply and relocation.
6. Tailgate trucks (`Support` class, cargo) and their AI.
7. C2 recovery vehicle, collection point, repair.
8. Commander AI resupply threshold (doctrine data).
9. Exit scenario (a long defence on `demo` or `uplands` where the defender runs out of main-gun rounds without resupply) with a scenario test, balance batch, DESIGN.md §13.1 and §14 entry, TODO ticks.

## 6. Defaults taken (change before slice 2 if wanted)

| # | Question | Default |
|---|---|---|
| 1 | Supply point an entity or a scenario item? | **Scenario item** (graphic with stock); trucks are the entities (slice 6) |
| 2 | Instant refill or transfer at a rate? | **Rate from data**, vehicle halted while served |
| 3 | Re-arm events: trigger action or own list? | **Own scenario list**, applied as generated commands (simple to edit, replays regenerate) |
| 4 | Repaired vehicles return to the fight? | **Yes**, after a data repair time at a collection point |
