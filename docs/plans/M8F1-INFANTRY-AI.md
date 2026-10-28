# M8f1 infantry AI: design plan

Draft 2026-10-28. TODO items F11 (reaction to infantry contacts), F5 (AI use of buildings) and
B13 (the ambush exit scenario); B33 (concealment in buildings) is folded in because B13 depends
on it. G8 (plans and card orders for squads) and G9 (editor placement) are already done, so
B13's prerequisites outside this plan are met. Decisions are in §7; nothing here is coded
until they are agreed.

## 0. Scope

Tank combat stays the theme (infantry plan §0). The AI work is the minimum that makes a village
fight play right:

- a tank platoon caught at close range by rockets gets out of the kill zone instead of halting
  in it;
- tank-only platoons stop driving into built-up areas once they know enemy infantry is about;
- commander-AI IFV platoons put their squads on the ground at a sensible time and place;
- defending squads use houses that cover the approach;
- a tech scenario (B13) shows the difference infantry support makes, with assertions.

Not in M8f1 (stays M8f2 or later): clearing a village room by room, squads bounding by fire
teams, combined-arms COA choice, OPFOR plans, cross and depth fire (F1–F4).

## 1. What exists

- Tank gunnery against infantry (B11): squads inside the doctrine's `close_infantry_m` come
  first, squads seen launching and identified missile teams before everything; coax in the
  open, HE behind walls.
- Contact drill (ATP 3-20.15 App. F): halt, face the threat, engage, continue after 45 s quiet;
  per-leg ON CONTACT can be CONTINUE, HASTY POSITION or BREAK CONTACT. Break contact smokes and
  withdraws away from the nearest threat.
- Squad drills: react to contact (cover within `cover_search_m`), ambush hold (`ambush`
  trigger ranges, prefer flanks), dismount and mount drills, occupy a building.
- Commander AI (`ai::commander`): attack (march, pre-battle, battle, assault position,
  consolidate), defend with alternates, reserve, recon. It orders platoons only; dismount
  platoons are never tasked and squads stay aboard (so calibration V4 cannot run).
- Plans can now dismount, mount and occupy buildings (G8); the plan runner is the natural way
  for the commander AI to sequence "move, dismount, occupy" without new order types.

## 2. Reaction to a close ambush (F11a)

Trigger: a member of a moving tank or IFV platoon is fired on by an anti-armour weapon (launch
seen, or a hit or near miss whose source is known) from inside the doctrine's new
`close_ambush_m` (proposed 400 m, beyond RPG-7 effective range), or the platoon loses a vehicle
to a shooter it cannot place.

Proposed drill (ATP 3-21.8 (2016) battle drill "React to ambush (near)" (drill number to
verify): leave the kill zone at once, return fire, suppress; ATP 3-20.15 App. F react to contact and break contact; Grau (1997)
on Grozny columns that halted in the kill zone):

1. Every vehicle fires its smoke grenades toward the shooter (existing self-screen).
2. Vehicles in the kill zone reverse along their own track (`Order::Reverse`, hull to the
   threat) until outside `close_ambush_m` of the shooter, or 150 m, whichever comes first.
   Reversing keeps the front armour on the threat; the route behind is ground already driven.
3. The platoon then fights from there under the contact drill: HE at the firing point, coax at
   squads seen. The mission does not resume until the contact has been quiet as today, or the
   player or commander orders otherwise.
4. Report: `CONTACT, AMBUSH, <grid>, BREAKING OUT` (brevity as the other reports).

A per-leg ON CONTACT of CONTINUE still drives through (the player's call); BREAK CONTACT keeps
its meaning. The drill is in the doctrine pack (`infantry.react_ambush`: radius, reverse
distance), both packs cite the same sources.

## 3. Tanks and built-up areas (F11b)

FM 3-06.11 (2002) *Combined Arms Operations in Urban Terrain* and ATP 3-06 *Urban Operations*
(edition to verify; neither is in REFERENCES.md yet): tanks do not enter built-up areas without
dismounted infantry; they overwatch from stand-off and support the infantry's clearing.

Proposed rule: once a side's picture holds an enemy infantry contact (any squad, or a launch),
routes planned for its tank-only platoons by the commander AI avoid built-up ground, i.e.
building cells plus a margin (proposed 50 m, so streets lined by houses count) cost as if they
were slow going. Platoons with their own squads on the ground near them (within 300 m), and
IFV platoons, are exempt. Player orders are not changed; the ghost order line warns
"BUILT-UP AREA, NO DISMOUNTS" when a player sends a tank-only platoon into one.

Before any enemy infantry is in the picture nothing changes, so the six v1 scenarios (no
squads) and the golden hash stay as they are.

## 4. When IFVs dismount (F11c)

Commander-AI tasks with carrier platoons get dismount points from the doctrine pack
(`infantry.dismount`):

- **Attack.** Western (ATP 3-21.71 (2010) ch. 3; ATP 3-90.1): dismount at a covered point short
  of the objective, out of direct fire, then the squads assault on foot while the IFVs
  overwatch. Generic OPFOR (FM 100-2-1 (1984) ch. 5): mounted attack by default; dismount
  300-400 m short of the objective when it is a built-up area or anti-tank weapons have been
  seen. Distances are data; the dismount point is the route point at that distance, moved to
  the nearest cover found by the existing position search.
- **On contact.** An IFV platoon that runs into a close ambush (§2) dismounts its squads after
  breaking out, as security and to clear the firing point (squads ordered to the shooter's last
  known position).
- **Defend.** Squads dismount into the buildings that cover the battle position (§5); the IFVs
  take the battle position.

The commander issues these as plans (G8 actions), so no new order types are needed and the
player sees the AI's plan as for any platoon. Mounting again: at consolidation (attack) and on
displacement (defend), with a MOUNT step.

## 5. Squads in buildings (F5)

- **Defend.** For each dismount platoon of a defending task, choose buildings within 300 m of
  the battle position that see the threat axis (a sightline from the facade toward the axis
  centre at 500 m, existing `facade_point` and LOS), prefer masonry and upper floors, fill by
  squad capacity; order OCCUPY BUILDING. Missile teams prefer the flanks of the BP, as their
  ambush doctrine already says.
- **Attack.** Squads move to the buildings on the objective's near edge, the IFVs and tanks
  overwatch from stand-off. Clearing inside the village is the squads' existing fire and
  occupation; no room-by-room model.
- **AT teams moving after firing** (deferred from M8b-b §9 #7): after firing twice from one
  building (or being fired on with HE) a team displaces to the next building chosen as above,
  away from the target. Optional in M8f1, see §7.

## 6. B13 ambush scenario and B33

B13 (`m8f-ambush`, tech demo on the `demo` village): one Red RPG team and one missile team in
houses, two MR squads in a third; Blue moves through the village in two variants:

- (a) a tank platoon alone;
- (b) the same tanks with a mech platoon that dismounts short of the village and leads.

Assertions over 16 seeds: (a) Blue loses a tank in most runs (≥ 10/16) and the tanks break out
of the kill zone (§2) rather than halting in it; (b) Blue's tank losses are at most half of (a)
and the squads find at least one team before it fires in most runs. Exact thresholds after a
first run, as for A10.

**B33** (waiting on the user since 2026-10-28): an alert tank finds a squad in a lone house at
350-450 m in about 6 s, because a squad in a building has no concealment factor (the
commander's unaided view resolves its window). Then variant (a) cannot show the ambush threat
with an alert crew. Proposed: `InfantryModel.camouflage.in_building` 0.5 on the optical and
night channels, as D1 did for dug-in squads (men fire from back in the room, FM 3-06.11), thermal
unchanged; re-run `detect`, the A10 check with an alert crew (B26) and
the batch. Golden hash unchanged (no squads in it).

## 7. Decisions

**Decided 2026-10-28** (user):

- **#1** Reverse out is the default; the player sets the SOP per leg: a new leg setting
  **ON AMBUSH** (REVERSE OUT / DRIVE THROUGH / HALT), plus an **AMBUSHED** condition for
  IF ON LEG branches (e.g. if ambushed, go to CP 5).
- **#2** Changed from the recommendation: tank-only platoons **always** avoid built-up ground,
  for commander-AI and player routes alike, from the start (not only once infantry is seen); a
  leg setting **BUILT-UP** (AVOID / ENTER) overrides it per leg. The v1 scenarios' routes
  change: golden hash update and balance re-run (and retune if needed). §3 and §9 below are
  superseded on this point.
- **#3, #4, #6** as recommended. **#5 (B33)** factor 0.5 as recommended.


| # | Question | Options | Recommendation and why |
|---|---|---|---|
| 1 | Close-ambush reaction (§2) | (a) reverse out of the kill zone, smoke, then fight; (b) drive through the kill zone; (c) keep today's halt-and-fight | **(a)**. Tanks in a village street cannot outrun the next house; reversing keeps the front armour on the shooter over ground already driven. (b) remains available as the leg's ON CONTACT CONTINUE |
| 2 | When do tank-only platoons avoid built-up areas (§3) | (a) commander-AI routes, once enemy infantry is in the picture; (b) always; (c) never (player and scenario designer decide) | **(a)**. Picture-based (no truth leak), and v1 scenarios without infantry are unchanged; player orders are only warned |
| 3 | IFV dismounting (§4) | (a) doctrine-pack dismount distances, issued as plans; (b) dismount only on contact; (c) leave squads to the player | **(a)**. Lets Red's BMP squads fight (and calibration V4 run) and uses the G8 plan actions, so no new order type |
| 4 | AT teams move after firing (§5) | in M8f1 / defer to M8f2 | **In M8f1, small** (two shots or HE, then the next covering building). Without it a known firing point is just HE'd; B13 is more honest with it |
| 5 | B33 concealment of squads in buildings (§6) | factor 0.5 on optical and night channels / another value / none | **0.5, as dug-in**, cited to FM 3-06.11; then re-check A10 with an alert crew. Without it B13 (a) shows no threat |
| 6 | Scope of the attack case (§5) | (a) squads take the near edge, vehicles overwatch; (b) also clear the village block by block | **(a)**. Block-by-block clearing needs towns (H3) and real urban AI; that is M8h1b and later |

## 8. Order of work (each about one commit)

1. B33 camouflage in buildings (data + one factor), `detect` re-run, A10 with an alert crew.
2. Close-ambush drill (§2): doctrine block, drill, report text, unit tests on a synthetic street.
3. Built-up avoidance (§3): cost layer for commander-AI routes, picture trigger, client warning.
4. Commander dismount points (§4): doctrine block, plans for carrier and dismount platoons,
   tests in `crates/ai/tests` and a sim scenario test.
5. Building choice for defending squads (§5), then AT team displacement if agreed.
6. B13 scenario and its 16-seed assertions; V4 calibration run; balance batch of the v1
   scenarios (must be unchanged); docs (DESIGN.md §13.1, §14, TODO).

## 9. Hash, replays, balance

- Golden hash: unchanged by design (no squads in the golden scenario; the avoidance needs an
  infantry contact). Any change is a bug to explain.
- `SIM_VERSION` bump with step 2 (new behaviour wherever squads exist).
- Balance: the 16-seed batch of the six v1 scenarios must be unchanged run for run;
  `m8b-dismounts` and `m8b-ambush` change and are re-checked.
