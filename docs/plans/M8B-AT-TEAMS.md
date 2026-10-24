# M8b-b anti-tank teams: design plan

Status: **draft for review, 2026-10-24; nothing implemented.** Covers TODO.md B2, B6, B9, B12 and
B19. Builds on M8b-a (`docs/plans/M8B-INFANTRY.md`) and the calibration work
(`docs/plans/M8B-CALIBRATION-NOTES.md`). §9 lists the decisions, each with a recommendation.

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

  The values are tuned so the published "effective range" is about a 50 % first-round hit:
  - RPG-7: about 500 m on a stationary tank-sized target, 300 m on a moving one;
  - M72: 200 m;
  - AT4: 300 m.

  This is *our* reading of "effective range" and is marked `estimate`.
- **Guided missiles.** The existing guided path is used (no lead or ballistic error, plus
  dispersion). Two additions:
  1. **Operator in the loop (SACLOS/MCLOS).** If the firing squad is pinned (suppression above
     `pinned_above`) or destroyed (zero strength) while its missile is in flight, the missile
     misses. This is why crews return fire at a missile post: suppressing the gunner is part of
     the react-to-ATGM drill (ATP 3-20.15 App. F). Fire-and-forget
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

### 2.3 Tandem warheads

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
- **Top armour data:** one `top_mm` stat per vehicle (KE and CE through `ce_factor`, as for the
  other faces), `estimate` with reasoning: usually 40–80 mm RHA equivalent on MBTs, 15–30 mm on
  IFVs. Dedicated top-attack submunitions and EFP mines stay in M9 T2. Only the value is
  brought forward.
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

| Side | Class | Mirrors | In service | Notes |
|---|---|---|---|---|
| Blue | light disposable | M72 LAW | 1963 | about 200 m effective (FM 3-23.25) |
| Blue | medium disposable | AT4 (M136) | 1987 | about 300 m (FM 3-23.25) |
| Blue | medium ATGM team | M47 Dragon | 1975–2001 | about 1,000 m, SACLOS (FM 23-24) |
| Blue | ATGM team | Javelin | 1996 | about 2,000 m, fire-and-forget, top attack (FM 3-22.37) |
| Red | RPG in squads | RPG-7 with PG-7V, PG-7VL (1977), PG-7VR tandem (1988) | 1961 | about 500 m stationary / 300 m moving (FM 100-2-3) |
| Red | light disposable | RPG-18, then RPG-22/26 | 1972 / 1985 | about 200 m |
| Red | ATGM team | 9K111 Fagot | 1970 | about 2,000 m, SACLOS |
| Red | ATGM team | 9K115 Metis | 1978 | about 1,000 m, SACLOS, two-man |
| Red | ATGM team | 9K135 Kornet | about 1998 in units | about 5,000 m, beam riding, tandem |

- Sources to add to REFERENCES.md:
  - FM 3-23.25 *Shoulder-Launched Munitions* (2001);
  - FM 23-24 *M47 Dragon* (1987, archive);
  - FM 3-22.37 *Javelin* (2003/2008);
  - FM 100-2-3 (1991);
  - Grau (FMSO 1997);
  - Grau and Thomas, "Russian Lessons Learned from the Battles for Grozny" (FMSO, 2000);
  - Army Recognition/Wikipedia-class summaries only for penetration figures, marked `low`.

## 7. Client

- Anti-armour symbol modifier on AT teams and on squads that carry launchers (APP-6D).
- A rocket or missile shot line, drawn like gun shots.
- The status card lists launchers and rockets left.
- AAR kill credit, which the shot path already gives.
- DATABASE: anti-armour rows on the squad sheets, `tandem` and `top_mm` on the ammo and vehicle
  sheets. Nothing else.

## 8. Hash, replays, balance

- Golden scenario: no squads, so it is unchanged by §2 and §4–5. **The tandem flag (§2.3) and
  `top_mm` (§3) touch existing vehicle-against-vehicle fights.** The RF-1 becomes tandem, and a
  shot from a ridge may plunge. So those commits update the golden hash deliberately and re-run
  the duel harness and the 16-seed balance batch.
- The plunge rule only applies to height differences above an angle that ridge fire at gun
  ranges never reaches: 20° at 1 km means 360 m of height. In practice it stays a
  village-and-building effect.
- `SIM_VERSION` bump (new behaviour for squads). Old replays are rejected cleanly, as decided
  in Z6.

## 9. Decisions (recommendations, for review)

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

## 10. Order of work (each line is about one reviewable commit)

1. Refactor `resolve` to take a `FireControlDef` and shooter height; golden hash must stay
   unchanged (proves the refactor).
2. Data: `AntiArmourDef`, ammo rows, `tandem`, `top_mm` on vehicles; loader tests; DATABASE
   rows.
3. Tandem rule; duel harness re-run; golden hash update (deliberate); balance batch.
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

| # | Target | Value |
|---|---|---|
| A1 | RPG-7 first-round hit on a stationary tank-sized target | about 0.5 at 500 m, about 0.9 at 100 m (FM 100-2-3 effective range, *derived*) |
| A2 | Same on a target crossing at 5 m/s | about 0.5 at 300 m |
| A3 | PG-7V against a T-72-class front / side | front not penetrated; side and rear penetrated |
| A4 | Upper-floor RPG against an MBT roof (plunge > 20°) | penetrated |
| A5 | PG-7VR against Kontakt-1-type ERA | ERA spent, penetration not reduced |
| A6 | SACLOS missile with the operator pinned during flight | misses |
| A7 | Exit scenario: RPG team in a house, tank passes at 100–150 m showing its flank | tank killed or disabled in most seeds (more than half of 16) |
