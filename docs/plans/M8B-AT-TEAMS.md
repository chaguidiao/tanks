# M8b-b anti-tank teams: design plan

Status: **agreed 2026-10-24** (all recommendations in §9 accepted). Calibrated against public
sources on 2026-10-24 (§12); nothing implemented yet. Covers TODO.md B2, B6, B9, B12 and B19.
Builds on M8b-a (`docs/plans/M8B-INFANTRY.md`) and the calibration work
(`docs/plans/M8B-CALIBRATION-NOTES.md`).

Goal: infantry becomes a real danger to tanks at close range and in villages. A Red RPG gunner
in a house can kill a tank from the flank. A missile team on a treeline makes tanks use ground
and smoke. Tanks answer with coax, HE, smoke and stand-off. Every result replays to the same
hash.

Exit (TODO.md):
- a scripted test where an RPG team in a village kills a tank from the flank;
- per-pairing unit tests;
- a balance batch.

## 0. Scope (from the infantry plan §0)

Tank combat is the theme. Anti-tank weapons are modelled **as threats to vehicles**: when they
fire, what they hit, what they penetrate and how the crew reacts. The following are **not**
modelled:
- the men carrying them, beyond what the squad element already models;
- reloading drills in detail;
- backblast danger to friendly troops;
- weapon handling.

Anything below that does not change a tank decision is to be cut.

## 1. What already exists

| Need | Already in the code | Gap |
|---|---|---|
| A projectile that flies, hits a face and penetrates | `gunnery::resolve` + `Projectile` (main guns, autocannon, vehicle missiles) | `resolve` takes the shooter's `VehicleDef`: it reads only `fire_control` from it |
| Guided missiles | `AmmoDef.guided` (no lead or ballistic error); SACLOS and MCLOS dispersion in data | the operator must keep the sight on target for the whole flight; nothing checks this today |
| HEAT against composite armour and ERA | `ce_factor`, `EraDef` (tiles, coverage, `ce_defeat`) | **no tandem warheads**: a PG-7VR or Kornet is stopped by ERA like a single charge |
| Faces and hull/turret | `face_of` (front/side/rear), impact height picks turret or hull | **no top**: a shot from an upper floor or a top-attack missile meets side or front armour |
| Damage tables | non-penetration, turret, hull front, hull side/rear | enough for B12 once top hits exist (§3) |
| Squads, postures, buildings, detection, firing cue | M8b-a (`SquadDef`, `infantry.rs`, `smallarms.rs`, `sensors.rs`) | squads carry only small arms |
| Tanks reacting | contact drill, auto smoke when targeted (`smoke::units`, `last_targeted`), threat order (`choose_target`) | the threat order knows "infantry inside close range", not "anti-tank team" |

So most of M8b-b is data plus a few small extensions of paths that already exist. Nothing new
is needed at the scale of the burst model.

## 2. Model

### 2.1 Where anti-tank weapons live

**One list on `SquadDef`: `anti_armour: Vec<AntiArmourDef>`**, dated like the small arms:

```text
AntiArmourDef { name, in_service, count, ammo: "<AmmoDef id>", rounds, fire_control: FireControlDef-lite,
                trigger_range_m (ambush), setup_s (tripod/launcher ready), signature_factor (backblast) }
```

- **Rifle squads carry their own short-range weapons.** The Red MR squad gets an RPG-7 class
  launcher with a few rockets. The Blue squad gets disposable launchers: M72 class, then AT4
  class.
- **Anti-tank teams are squad types of their own**, with 2–3 men and an ATGM or a heavy RPG:
  `at_team_b` (Dragon class, then Javelin class) and `at_team_r` (Fagot/Metis class, then
  Kornet class). They are dismounted squads in every other way: posture, buildings, detection
  and losses.
- Ammunition is ordinary `AmmoDef` rows in `data/ammo/`: kind `heat`, `guided` or not, with
  penetration, flight speed and dispersion. No new resolution model is needed.

### 2.2 Firing (B2, B9)

- Squads fire through the existing `Projectile` path. `resolve` changes to take a
  `&FireControlDef` and the shooter's height instead of a `VehicleDef`, so a squad can be the
  shooter. This refactor keeps the order of RNG draws, so the golden hash stays as it is.
- **Unguided rockets (RPG, LAW, AT4).** The fire-control block gives:
  - aim error;
  - a range estimation error as a fraction of range (no rangefinder);
  - a lead error fraction for moving targets;
  - ballistic error, which stands in for crosswind sensitivity (wind is not modelled per shot
    in v1).

  **Calibration (§12):** published "effective range" is **not** a 50 % hit range. For the
  RPG-7, test tables give 91 % at 100 m, 52 % at 200 m, 27 % at 300 m and 5 % at 500 m on a
  stationary tank-sized target (Isby 1988). Against a crossing target at 4 m/s, a 1975 US Army
  evaluation gives 96 / 51 / 22 / 4 % at 100 / 200 / 300 / 500 m. Hit chance falls faster than
  a constant aiming error would give. The cause is range estimation: US tests found trained
  gunners 10–15 % off in range, and on a slow rocket's curved path that becomes a vertical miss
  that grows with range. So the physics is kept:
  - the range error goes through the round's drop (`estimated_range_error_fraction` 0.12,
    source: US tests);
  - the aim error and a ballistic error carry wind and launch effects;
  - each launcher is tuned to its table with the harness.
  The M72 and AT4 have no public hit tables, only effective ranges (M72 200 m static / 100 m
  moving, AT4 300 / 150 m, FM 3-23.25 via a secondary table). They are tuned to the same curve
  shape so that the effective range is about a 25–30 % hit, as it is for the RPG-7
  (*derived*).
- **Guided missiles.** The existing guided path is used (no lead or ballistic error, plus
  dispersion). Two additions:
  1. **Operator in the loop (SACLOS/MCLOS).** If the firing squad is pinned (suppression above
     `pinned_above`) or destroyed (zero strength) while its missile is in flight, the missile
     misses. This is why crews return fire at a missile post: suppressing the gunner is part of
     the react-to-ATGM drill (ATP 3-20.15 App. F). The 1973 evidence is strong: the Sagger's hit
     rate fell from about 25 % in Egyptian hands at the start to about 2 % for the Syrians at
     the end. By then Israeli crews fired at every launch point and kept moving (Iyer 1995, via
     Wikipedia). Proving-ground figures (90 % at 500–3,000 m) were never reached in combat. Fire-and-forget
     (Javelin-class, `guidance: fire_and_forget`) is immune.
  2. **Minimum range and setup time** (`setup_s`). A tripod ATGM team cannot fire on the move
     or straight after halting.
- **Ambush rule (B9 hold fire until close).** A squad holds its anti-armour fire until either:
  - a recognised vehicle comes inside `trigger_range_m` (data, from the doctrine pack: RPG
    about 150–200 m, ATGM about its effective range), or
  - the squad itself is fired on.

  It prefers a target showing a side or rear face. When several are inside range it picks the
  one with the best penetration margin, using the same rule as `select_round`. Under WEAPONS
  HOLD it never fires; under WEAPONS TIGHT the target must be identified.
- **Backblast reveals the shooter.** A launch sets `last_fired` with an extra
  `signature_factor` on the existing firing cue, visible to eyes and thermal sights. The target
  vehicle's `last_targeted` is set, so the auto-smoke drill fires as it does today for a gun
  shot.

### 2.3 Behind-armour effect (new, from the calibration)

Today any penetration rolls the same damage table. For a T-72, that gives a catastrophic kill
in about 60 % of penetrations (0.30 × the exposed-carousel factor 2).

The evidence says a light shoulder-fired HEAT penetration is much less lethal than that:
- In Grozny, each destroyed vehicle took **3–6 lethal hits** on average, from five or six
  hunter-killer teams firing at one vehicle (Grau 1997).
- In Iraq in 2003, only two Abrams were disabled by enemy fire in the major combat phase. RPG
  hits through the side skirts and into the engine compartment mostly caused mobility kills
  (PM Abrams report via Jane's).

The reason is physical: a 40–90 mm warhead's jet makes a narrow path behind the armour and
kills only what it meets. So:
- `AmmoDef.behind_armour` (default 1.0) scales the catastrophic, firepower and crew outcomes of
  a penetration. The difference becomes a hit with no lasting effect. Mobility outcomes keep
  their full value: hits on the engine and running gear still stop the vehicle.
- Light rockets (RPG-7, M72, AT4, RPG-18/22/26) get about 0.4. Large ATGMs and 105–125 mm HEAT
  keep 1.0.
- `estimate`, calibrated to "about 3 hits to destroy" on a T-72 side with PG-7V class rockets
  (target A8 in §11).
- Existing rounds keep 1.0, so the golden hash is not changed by this.

### 2.4 Tandem warheads

`AmmoDef.tandem: bool` (default false). A tandem charge's precursor fires the ERA tile: the tile
is spent, but `ce_defeat` does not apply. This covers the PG-7VR (1988), RPG-29 class, Kornet,
TOW-2A/B and the gun-launched RF-1 (data/ammo/atgm_refleks.ron already says "tandem"). It changes
existing duels where a tandem round meets ERA, so the duel harness and balance batch are re-run
(§8).

## 3. Close-in vulnerability (B12)

What actually kills tanks at close range in built-up areas is hits on the thinner faces. Grau,
*Russian-Manufactured Armored Vehicle Vulnerability in Urban Combat: The Chechnya Experience*
(FMSO, 1997), records where Russian vehicle hits in Grozny landed: flanks, rear and the tops of
turrets and engine decks, fired from upper floors and basements. Front-armour hits were rare.

- **Side and rear** are already geometric (`face_of`). An ambush that holds fire until the
  flank shows gets the side values by itself. No "side/rear-weighted hit chance" is needed as
  a separate rule.
- **Top (plunging) hits are new.** If the shooter is above the target by more than an angle
  (data, about 20°: an upper floor at close range), or the missile is top-attack, the round
  meets a new `top` armour value. Its obliquity is the plunge angle, and the turret or engine
  deck table applies.
- **Top armour data:** one `top_mm` stat per vehicle. It is plain steel, so `ce_factor` does
  not apply. No authoritative public figure exists; the only roof values found are from a game
  database (T-72B: turret roof about 45 mm, engine deck about 20 mm). Values are `estimate`:
  - MBTs 30–50 mm;
  - IFVs 15–30 mm.

  **The result is insensitive to them.** Every anti-tank warhead here penetrates 260 mm or
  more, so any top hit penetrates whatever the value. The value matters only for autocannon
  and HE fragments, and that is acceptable at low confidence. Dedicated top-attack submunitions
  and EFP mines stay in M9 T2.
- **Damage tables:** a top penetration uses `turret_penetration` (turret roof) or
  `hull_side_rear_penetration` (engine deck), chosen by impact point as today. No new table.
- **Deferred:**
  - a minimum arming distance (RPG-7 about 2–5 m): no fight in v1 is that close;
  - mobility kills on tracks and running gear from small HEAT: already in the damage tables'
    mobility share.

## 4. Detection and identification

- AT teams use the squad posture rows; a 2–3-man team is a smaller group (data).
- `ContactKind::Infantry` stays. At **Identified**, a contact shows the type name (e.g. "AT
  TEAM (ATGM)"), as squads already do. The APP-6D anti-armour modifier is drawn on the symbol
  (client).
- Backblast (§2.2) is the main way a hidden team is found. With camouflage (B24 D1), a dug-in
  team should mostly be found **when it fires**, and the search-time step (D2) sharpens this
  further.

## 5. Tank AI against anti-tank teams (B6)

Kept small. The bigger questions belong to F11/F5: whether to enter villages without infantry,
and clearing with dismounts.

- **Threat order (`choose_target`).** A contact known to have fired an anti-armour weapon
  (recent backblast) or identified as an AT team comes **first**, ahead of vehicles. Other
  infantry inside the close range comes next, as today.
- **Weapon choice.** Already in place: coax at men in the open, HE at men behind walls. The
  only change is that a missile post is engaged even beyond coax reach, with HE.
- **ATGM stand-off (B6).** Vehicles whose best anti-tank round is a missile engage from beyond
  the gun range where the doctrine pack allows. Missile-armed IFVs (M2/BMP-2 class) already
  hold fire until in range. This adds: the vehicle prefers its missile beyond the gun's
  effective range, and the gun inside it. That is a data rule in `select_round` (a
  `prefer_beyond_m` on the loadout), not new AI.
- **React to ATGM** (ATP 3-20.15 App. F). The existing contact drill and auto smoke cover it
  once `last_targeted` is set by a launch. No new drill.

## 6. Doctrine and data (B19)

- `generic-opfor` gains an `ambush` block in `infantry`: trigger ranges per weapon class, hold
  fire until the first vehicle is inside the kill zone, preferred aspect. Cited: FM 100-2-1
  (1984) ambush, Grau (1997), ATP 3-21.8 (2016) for the hasty ambush drill.
- `western-combined` gets the same block with its own values: ATP 3-21.8 ch. 7 ambush; FM
  3-23.25 for the shoulder-launched weapons.
- Era sets (one per class and period; all `low`/`estimate` from public sources, fictional
  names):

| Side | Class | Mirrors | In service | Calibrated public data (2026-10-24) |
|---|---|---|---|---|
| Blue | light disposable | M72 LAW | 1963 | 200 m static / 100 m moving effective (FM 3-23.25 via a secondary table); penetration about 300 mm (`low`) |
| Blue | medium disposable | AT4 (M136) | 1987 | 300 m static / 150 m moving; "more than 14 in" (356 mm) penetration (FM 3-23.25) |
| Blue | ATGM team | M47 Dragon | 1975–2001 | 1,000 m, 11 s flight, SACLOS; 330 mm; Dragon II about 610 mm (dates to check) |
| Blue | ATGM team | Javelin | 1996 | 65 m (direct) / 150 m (top attack) to 2,000 m; about 140 m/s; penetration classified, "over 760 mm" quoted (`low`); fire-and-forget |
| Red | RPG in squads | RPG-7: PG-7V 1961, PG-7VL 1977, PG-7VR tandem 1988 | 1961 | hit table in §2.2 (Isby 1988); 260–330 / about 500 / 600 mm behind ERA (750 without) |
| Red | light disposable | RPG-18, then RPG-22/26 | 1972 / 1985 | RPG-22 about 160 m, about 400 mm (secondary); RPG-26 to check |
| Red | ATGM team | 9K111 Fagot | 1970 | 75–2,000 m, 400 mm; later 9M111M 2,500 m, about 600 mm |
| Red | ATGM team | 9K115 Metis | 1978 | 40–1,000 m, 460 mm |
| Red | ATGM team | 9K135 Kornet | 1998 in service (shown 1994) | 100–5,500 m, beam riding, tandem; 1,000–1,200 mm behind ERA |

- Sources to add to REFERENCES.md:
  - FM 3-23.25 *Shoulder-Launched Munitions* (2001);
  - FM 23-24 *M47 Dragon* (1987, archive);
  - FM 3-22.37 *Javelin* (2003/2008);
  - FM 100-2-3 (1991);
  - Grau (FMSO 1997);
  - Grau and Thomas, "Russian Lessons Learned from the Battles for Grozny" (FMSO, 2000);
  - Isby, *Weapons and Tactics of the Soviet Army* (Jane's, 1981/1988) for the RPG-7 hit
    table (via a forum transcription; check against a copy);
  - Army Recognition/Wikipedia-class summaries only for penetration figures, marked `low`.

## 7. Client

- Anti-armour symbol modifier on AT teams and on squads that carry launchers (APP-6D).
- A rocket or missile shot line, drawn like gun shots.
- The status card lists launchers and rockets left.
- AAR kill credit, which the shot path already gives.
- DATABASE: anti-armour rows on the squad sheets, `tandem` and `top_mm` on the ammo and vehicle
  sheets. Nothing else.

## 8. Hash, replays, balance

- Golden scenario: no squads, so it is unchanged by §2 and §4–5. **The tandem flag (§2.4) and
  `top_mm` (§3) touch existing vehicle-against-vehicle fights.** The RF-1 becomes tandem, and a
  shot from a ridge may plunge. So those commits update the golden hash deliberately and re-run
  the duel harness and the 16-seed balance batch.
- The plunge rule only applies to height differences above an angle that ridge fire at gun
  ranges never reaches: 20° at 1 km means 360 m of height. In practice it stays a
  village-and-building effect.
- `SIM_VERSION` bump (new behaviour for squads). Old replays are rejected cleanly, as decided
  in Z6.

## 9. Decisions (agreed 2026-10-24: all recommendations accepted)

| # | Question | Options | Recommendation and why |
|---|---|---|---|
| 1 | Where do anti-tank weapons live? | (a) a list on `SquadDef`, used by rifle squads and AT-team squad types; (b) a new entity type | **(a)**. One element model (§0); AT teams get posture, buildings, detection and losses for free |
| 2 | How is an RPG or missile shot resolved? | (a) the existing projectile and `resolve` path with a fire-control block; (b) a burst-style draw | **(a)**. Faces, ERA, penetration and damage tables already exist and are validated; a draw would duplicate them less accurately |
| 3 | Red MR squad: RPG gunner as an 8th man, or one of the 7 carries it? | 8 men (BMP-1 has 8 seats, BMP-2 7) / 7 men | **7 men, one rifle swapped for the RPG**. Fits both carriers; FM 100-2-3 lists the RPG-7 gunner inside the squad |
| 4 | Top armour now, or keep it for M9? | (a) one `top_mm` per vehicle now, used for plunging shots and top-attack missiles; (b) proxy with rear values; (c) nothing, Javelin as a side hit | **(a)**. Hits from upper floors are the main village threat (Grau 1997), and Javelin is in the 2005 scenarios; one stat per vehicle is cheap. EFP and submunitions stay M9 |
| 5 | Tandem warheads? | add `tandem` / leave them out | **Add**. Without it, PG-7VR and Kornet are wrongly stopped by Kontakt-type ERA, and the RF-1's own note is ignored. Costs a golden hash change and a balance re-run |
| 6 | Operator in the loop for SACLOS? | miss if the team is pinned or destroyed during flight / ignore | **Miss**. It is the reason tanks fire at a launch point; cheap, since suppression already exists |
| 7 | Ambush behaviour | hold fire until trigger range or fired on, prefer flanks (data) / also displace after firing | **Hold and prefer flanks only.** Shoot-and-move needs alternate positions: that is F-phase AI (F11) |
| 8 | Firing from inside buildings (backblast) | allowed for all / only "enclosure-safe" weapons | **Allowed for all**, abstracted. RPGs were routinely fired from rooms and basements in Grozny (Grau and Thomas 2000); a rule would cut the main ambush case |
| 9 | Content | new tech demo `m8b-ambush` (village, Red AT teams and MR squads in houses, Blue tank platoon); AT teams in v1 scenarios later in M8i / now | **Tech demo now, v1 scenarios later.** The v1 balance stays independent of AI that does not yet use infantry (F11) |
| 10 | Crosswind on unguided rockets | in the ballistic error (data) / per-shot from the scenario wind | **In the ballistic error for v1.** A per-shot wind term is a small later refinement (ENG backlog) |
| 11 | Behind-armour effect of small warheads (added with the calibration) | per-round `behind_armour` factor on kill outcomes / one table for all rounds | **Per-round factor** (§2.3). Without it one RPG penetration kills a T-72 60 % of the time, against 3–6 hits per kill in Grozny |
| 12 | Active protection and drones (added with the calibration) | model now / defer | **Defer.** The scenarios end in 2005. Drozd (1983) was fitted only to a few T-55AD; Arena and Trophy came later; loitering munitions are outside the era. Noted in TODO.md as later items |

## 10. Order of work (each line is about one reviewable commit)

1. Refactor `resolve` to take a `FireControlDef` and shooter height; golden hash must stay
   unchanged (proves the refactor).
2. Data: `AntiArmourDef`, ammo rows, `tandem`, `top_mm` on vehicles; loader tests; DATABASE
   rows.
3. Tandem rule and `behind_armour`; duel harness re-run; golden hash update (deliberate); balance batch.
4. Top/plunging hits; unit tests: an upper-floor RPG penetrates an MBT roof and not its front;
   golden hash if changed.
5. Squad anti-armour fire: setup time, trigger range, aspect preference, backblast cue,
   operator in the loop; per-pairing unit tests: RPG-7 against T-72 side and front, AT4 against
   BMP, Dragon/Javelin against T-72, Fagot/Kornet against the M1 family. Hit-probability curves
   go in a `tanks-headless duel --shooter-squad` mode or a `firefight --target <vehicle>`
   variant.
6. Tank AI: threat order for AT teams and launch points; HE at missile posts; missile stand-off
   rule (B6).
7. Doctrine `ambush` blocks with citations; squad and team data per era (B19).
8. Scenario tests (synthetic village): an RPG team in a house kills a tank from the flank; a
   tank platoon with HE clears a known missile post; a pinned operator's missile misses. Tech
   demo `m8b-ambush`.
9. Client: symbol modifier, shot lines, status card, AAR; screenshot check.
10. Docs: DESIGN.md §13.1 and §14 entries, TODO ticks, calibration notes for the AT numbers.

## 11. Validation targets

| # | Target | Value | Source |
|---|---|---|---|
| A1 | RPG-7 (PG-7V) first-round hit, stationary tank-sized target, no wind | 0.91 / 0.52 / 0.27 / 0.05 at 100 / 200 / 300 / 500 m (± 0.08) | Isby (1988) |
| A2 | Same, target crossing at 4 m/s | 0.96 / 0.51 / 0.22 / 0.04 at 100 / 200 / 300 / 500 m (± 0.08) | US Army evaluation 1975–76 |
| A3 | M72 / AT4 hit at their static effective range | about 0.25–0.30 | FM 3-23.25 ranges, same curve shape as A1 (*derived*) |
| A4 | PG-7V against a T-72-class front / side | front not penetrated; side and rear penetrated | penetration data |
| A5 | Upper-floor RPG against an MBT roof (plunge over 20°) | penetrated | §3 |
| A6 | PG-7VR against Kontakt-type ERA | tile spent, penetration not reduced | §2.4 |
| A7 | SACLOS missile with the operator pinned during flight | misses | §2.2; Sagger 1973 |
| A8 | PG-7V-class hits on a T-72 side until it is destroyed (catastrophic or firepower kill) | mean 2–4 hits | Grau 1997 (3–6 lethal hits per destroyed vehicle, all aspects) |
| A9 | Fire-and-forget missile (Javelin class) hit rate at 500–2,000 m, target in the open | about 0.85–0.9 | DoD, Ukraine 2022: 100 of 112 |
| A10 | Exit scenario: RPG team in a house, tank passes at 100–150 m showing its flank | tank killed or disabled in more than half of 16 seeds | scenario test |

## 12. Calibration against public data (2026-10-24)

What matters physically, and what is abstracted statistically:

| Aspect | Treatment | Why |
|---|---|---|
| Rocket flight, drop and range estimation | **Physical**: the existing ballistic path with an estimated-range error | It produces the steep fall in hit chance with range in the test tables. A flat statistical table would lose it, and it would lose the effect of a laser rangefinder on later launchers |
| Wind, launch jitter, gunner stress | **Statistical**: aim and ballistic error, plus the existing suppression growth | No per-shot data; wind is folded in (decision 10) |
| Combat against proving ground | **Through suppression, not a flat factor** | Sagger 25 % → 2 % came from crews firing at launch points, which the model has: suppression and the operator-in-the-loop rule. A flat "combat factor" would double-count it |
| Hit location: face, turret or hull, plunge | **Physical** (geometry) | The point of ambush doctrine. Grozny kills were 98 % on areas without ERA; the WWII T-34 survey has about half of all hits on the hull sides |
| Behind-armour damage | **Statistical**: per-round factor on the damage table | Jet-path modelling is beyond the scope; the factor reproduces "3–6 hits per kill" |
| Roof thickness | **Low-confidence estimate** | No public source, and the result does not depend on it (§3) |
| Penetration figures | **Open-source values, `low`** | Manufacturer and encyclopedia figures disagree by about 20 %, which is inside the margin that decides most pairings |

Sources used (secondary ones marked as such in the data):
- Isby (1988), RPG-7 hit table, via a [Battlefront forum transcription](https://community.battlefront.com/topic/91032-why-use-an-m136at4-when-you-have-a-javelin/page/2) (secondary);
- [generalstaff.org historical Ph/Pk page](https://generalstaff.org/BBOW/Weapons/Hist_PHitsPKills.htm) (secondary);
- [GlobalSecurity RPG-7](https://www.globalsecurity.org/military/world/russia/rpg-7.htm);
- Grau, [Russian-Manufactured Armored Vehicle Vulnerability in Urban Combat](https://man.fas.org/dod-101/sys/land/row/rusav.htm) (FMSO, 1997);
- Grau, [Russian Urban Tactics: Lessons from the Battle for Grozny](https://apps.dtic.mil/sti/pdfs/ADA394517.pdf);
- Iraq 2003 Abrams damage, via [Army Rumour Service](https://www.arrse.co.uk/community/threads/americans-dont-know-what-hit-them.3525/post-53095) and [StrategyPage](https://www.strategypage.com/htmw/htarm/articles/20030410.aspx) (secondary);
- [Manual command to line of sight](https://en.wikipedia.org/wiki/Manual_command_to_line_of_sight) (Sagger 1973, citing Iyer 1995);
- Javelin: [National Interest](https://nationalinterest.org/node/213728) (DoD 112 shots, 100 hits); [CSIS Missile Threat](https://missilethreat.csis.org/missile/fgm-148-javelin/); [Wikipedia](https://en.wikipedia.org/wiki/FGM-148_Javelin);
- [M47 Dragon](https://en.wikipedia.org/wiki/M47_Dragon), [9K111 Fagot](https://en.wikipedia.org/wiki/9K111_Fagot), [AT-7 Metis](https://www.globalsecurity.org/military/world/russia/at-7.htm), [9M133 Kornet](https://en.wikipedia.org/wiki/9M133_Kornet), [Kontakt-5](https://en.wikipedia.org/wiki/Kontakt-5), [AT4](https://en.wikipedia.org/wiki/AT4) (FM 3-23.25 penetration), [RPG-22](https://modernfirearms.net/en/grenade-launchers/russia-grenade-launchers/rpg-22-eng/);
- FM 3-23.25 *Light Antiarmor Weapons* (2001), [archive.org copy](https://archive.org/download/MManuals/UsArmyLightAntiarmorWeapons2001.pdf);
- hit distributions: [1942 T-34 damage survey](https://tanks-encyclopedia.com/1942-combat-damage-analysis-of-the-t-34-and-t-70-tanks/).

Still to check, before the data commit:
- the Isby table against a copy of the book;
- Dragon II and 9M111M service dates;
- RPG-18 and RPG-26 penetration;
- M72A2 penetration.
