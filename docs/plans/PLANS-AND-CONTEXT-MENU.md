# Execution plan: checkpoint plans and the map context menu

Status: **done** (P0–P5, 2026-10-14; DESIGN.md §13.1 lists what shipped and what is deferred). Approved 2026-10-13 as the hand-off document for the implementing agent.
Read `CLAUDE.md`, `docs/DESIGN.md` §8 and §10 and decisions #14 and #16 (§14) first.

## 1. Goal

The player can give a platoon a **plan**: a route of checkpoints where each leg and each
checkpoint carries its own instructions. For example:

> CP 1 at MAX in column, CP 1→2 at 15 km/h in wedge with traveling overwatch, at CP 2 occupy a
> hull-down BP facing 045° and wait for code word HAMMER, then move to CP 3. If contact on the
> leg to CP 3: smoke and break contact back to CP 2.

The plan is edited on the map with a **right-click context menu** and a checkpoint inspector.
Quick orders (hotkeys, command card, plain right-click move) stay as they are; a quick order on
a platoon replaces its plan, which is how you take back control when things get messy.

References for the interaction model:
- **TacOps** (Holdridge): orders are attached to waypoints in a unit's path; the Orders view
  shows an estimated time next to each waypoint. We take the ETA labels.
  ([TacOps user guide](https://generaltzsoschickencoop.wordpress.com/wp-content/uploads/2009/08/tacops4-guide-user.pdf))
- **Steel Beasts Pro PE**: a unit reaching a waypoint adopts that waypoint's tactics, formation,
  spacing, speed and fire control; tactics include the reaction to contact (stop, withdraw,
  find hull-down and engage, rush to the next waypoint); routes leave a battle position when a
  condition is met. We take per-leg settings, per-leg contact reaction and conditional
  continuation. ([SBWiki: Moving units on routes](https://www.steelbeasts.com/sbwiki/index.php/Moving_Units_On_Routes))
- **Doctrine**: checkpoints and routes (FM 1-02.2, ADP 3-90); "on order" execution by code word
  and the execution matrix (FM 6-0); react to contact and break contact (ATP 3-20.15 App. F).
  Add TacOps and FM 6-0 to `docs/REFERENCES.md` in phase P5.

Terminology in the UI: **CP** (checkpoint), never "waypoint": the game already says checkpoint.

## 2. Design decisions (record as DESIGN.md §14 #22–#24 in P5)

**#22 A plan is one radio order.** `Command::SetPlan` crosses the net once (radio delay as any
order; immediate in the planning phase), like decision #16 for routes. The platoon AI owns the
plan state (cursor, waiting, branch) so it survives the contact drill, HALT/RESUME and replays.

**#23 The plan runner reuses existing orders.** A plan step does not get new movement code. A
runner in the 1 Hz platoon phase turns the current step into the existing orders
(`PlatoonMove` with `via`, `SetSpeed`, `SetFormation`, `WeaponsControl`, `OccupyPosition`,
`Smoke`, `FireSmoke`, `CallForFire`, `BreakContact`) and applies them directly in the same tick
(not through the radio queue). These applications are marked as coming from the plan so they do
not cancel the plan (see §3.4). They are `generated`, so replays regenerate them (decision #14).

**#24 Conditions are one vocabulary.** Plan waits and branches use the same
`TriggerCondition` as contingency triggers (`control.rs`), extended. Every condition reads the
side's `PerceivedWorld` only (fog of war, rule 3). Conditions combine as a flat list with
ANY/ALL; no nesting, no loops, no variables in v1.

Out of scope for this plan: single-vehicle plans (platoons only; a lone vehicle keeps
`Move`+`via`), nested logic, plan templates/playbook, an execution matrix screen, OPFOR use of
plans, multi-platoon selection. List them as deferred in DESIGN.md §13.1.

## 3. Sim design

### 3.1 Data (in `ai::picture`, serde, shared by sim and client)

```rust
pub struct Plan { pub steps: Vec<PlanStep> }

pub struct PlanStep {
    /// Stable id, unique within the plan (survives edits, used for `start` and branches).
    pub id: u16,
    pub at: Vec2,
    /// Settings for the leg that ends at this CP. None = keep what the platoon has.
    pub leg: LegSettings,
    /// Done on arrival, in order (instant actions; OccupyBp completes when set).
    pub actions: Vec<StepAction>,
    /// Hold at this CP until met. None = continue at once (pass-through if no actions).
    pub wait: Option<ConditionSet>,
    /// Done when the wait is met, before departing (the "on HAMMER: smoke, fire, go" part of a
    /// synchronised action). Same action set as `actions`.
    pub on_release: Vec<StepAction>,
    /// While on the leg to this CP or waiting at it: if met, jump to step `goto`.
    pub branch: Option<Branch>,
}

pub struct LegSettings {
    pub speed: Option<SpeedSetting>,            // Max | Limit(m/s)
    pub formation: Option<Formation>,
    pub technique: Option<Technique>,
    pub weapons: Option<WeaponsStatus>,
    pub on_contact: Option<ContactReaction>,
}

pub enum ContactReaction {
    /// Doctrine-pack drill (default): halt, orient, engage, continue when quiet.
    Drill,
    /// Keep moving, return fire on the move (bypass).
    Continue,
    /// Find hasty hull-down positions near the current location facing the threat
    /// (reuse the BP finder) and engage.
    HastyPosition,
    /// Battle drill break contact (existing `break_contact`).
    BreakContact,
}

pub enum StepAction {
    Face { bearing: f64 },
    OccupyBp { radius_m: f64, facing: f64, kind: PositionKind },
    SmokeScreen,                       // grenade salvo
    EngineSmoke(bool),
    FireSmoke { at: Vec2, rounds: u32 },
    CallForFire { battery: u32, at: Vec2, kind: FireKind, volleys: u32 },
    Report,                            // "SET AT CP n" radio report
}

pub struct ConditionSet { pub all: bool, pub conditions: Vec<TriggerCondition> }
pub struct Branch { pub when: ConditionSet, pub goto: u16 }
```

Move `PositionKind`/`FireKind` into `ai::picture` (re-export from `sim::command`) if needed to
keep `ai` free of `sim` types. Mark all new enum fields `#[serde(default)]` where it helps
hand-written RON.

`TriggerCondition` gains (existing variants unchanged, so old replays still load):
- `CodeWord { word: u8 }` — the side has sent that code word.
- `Elapsed { s: f64 }` — seconds since the step started waiting (plan-only; a trigger rejects it).
- `ContactWithin { platoon, range_m }` — a fresh contact within range of the platoon's centroid.
- `EnemyInArea { graphic: u32 }` — fresh contact inside an objective/BP graphic.
- `StrengthAtMost { platoon, remaining: u8 }`.
- `UnderFire { platoon }` — a member hit or a burst near the platoon in the last N s. The picture
  has no such field today: add `last_hit_s: Option<f64>` (own report, not truth) to `OwnUnit` and
  reuse the react-to-indirect-fire detection, or derive from platoon reports.
- `PlatoonAt { platoon, step: u16 }` — another platoon's plan has passed that step
  (synchronisation: "when 1 PLT reaches CP 3, 2 PLT moves").

### 3.2 Commands

```rust
/// Give a platoon a plan, starting at step `start` (FM 6-0 execution by phases/code words).
SetPlan { platoon: PlatoonId, plan: Plan, start: u16 },
/// Release a code word: plans and triggers waiting on it go (pre-planned: no radio delay).
CodeWord { word: u8 },
/// Reaction to contact for the platoon's current and later legs (quick order too).
SetContactReaction { platoon: PlatoonId, reaction: ContactReaction },
```

`CodeWord` is a control-like command (no radio delay) like `AddTrigger`; add it to the
`control` partition in `Sim::settle` and `Sim::step` only if it must also work in the planning
phase (probably not: it is an execution-time signal). Code-word names come from data: add
`code_words: [..]` to the doctrine pack (e.g. HAMMER, ANVIL, SABRE, ...); the sim stores the
index, the client shows the name.

### 3.3 Runner (`sim/src/plan.rs`, called from `platoon_ai::step`, 1 Hz)

Platoon gains `plan: Option<PlanState>` (`plan`, `cursor` (index), `phase`
(Leg | Acting | Waiting{since_tick}), `passed: Vec<u16>`) and `contact_reaction`.

Per platoon, each 1 Hz tick, in platoon order (determinism):
1. If HALTED (held order) the plan is paused: do nothing.
2. Branch: if the current step's `branch.when` is met, set cursor to `goto`, phase Leg, emit
   `PlatoonReport::PlanBranch`.
3. Leg: when entering a leg, apply its settings, then issue one `PlatoonMove` to the next CP that
   is a **stop** (has actions, a wait, or a branch target), with all pass-through CPs as `via`
   so the formation flows through them (decision #16 machinery). Settings that change at a
   pass-through CP are applied when the virtual leader's `progress` passes that checkpoint's arc
   length (`PlatoonOrder::Move.checkpoints`); `SetSpeed`/`SetFormation` already work on a moving
   platoon — verify `SetFormation` does, fix if not.
4. Arrival = the platoon's move completed at that CP (order back to None after `SetAt`). The
   contact drill swaps the order to `Contact` and back to `Move` to the same destination; the
   runner must not treat that as arrival or as a new order.
5. Acting: run `actions` in order; `OccupyBp` completes when the BP order reports set
   (`reported: true`). Then Waiting if `wait` is Some, else next step.
6. Waiting: report `AtCheckpoint { waiting: true }` once, then when `wait` is met run
   `on_release` (same rules as `actions`) and advance. Last step done → plan finished, report.

Contact reaction: `contact_drill` reads `p.contact_reaction` (default `Drill` = today's
behaviour, so existing tests and the golden hash do not change). `Continue` skips the halt;
`BreakContact` calls the existing break-contact path; `HastyPosition` issues an `Occupy`
centred on the centroid (radius from doctrine, e.g. 150 m) facing the threat.

### 3.4 Interaction with other orders (invariants, test each)
- A player or trigger `PlatoonMove`, `OccupyPosition`, `BreakContact` or `Move` of a member
  addressed to the platoon **discards the plan**.
- `Halt` pauses the plan; `Resume` continues the same step.
- Posture orders (`SetSpeed`, `SetFormation`, `WeaponsControl`, `Smoke`, `SelectAmmo`,
  `SetContactReaction`) apply and keep the plan; the next leg with an explicit setting
  overrides them.
- Plan-issued applications must not discard the plan: apply them through a helper with a
  `from_plan` flag (or a `#[serde(skip)]` field on `TimedCommand`); never serialise it.
- A new `SetPlan` with `start` = id of the step being executed keeps progress (the client sends
  this when the player edits a running plan).

### 3.5 Views, events, hash
- `PlatoonView` gains `plan: Option<PlanView { plan, cursor_id, waiting: Option<ConditionSet>,
  passed }>`; `task` reads e.g. `PLAN CP 3/5`, `WAITING HAMMER`.
- `PlatoonReport` gains `AtCheckpoint { n, waiting }`, `PlanBranch { to }`, `PlanComplete`.
  `Event` gains `CodeWordSent { side, word }`.
- `state_hash` includes plan cursor, phase and contact reaction. No new RNG.
- The golden hash in `crates/sim/tests/m1.rs` must **not** change; if it does, a default changed.

## 4. Client design

### 4.1 Right-click semantics
| Input (no tool active) | Today | After |
|---|---|---|
| RMB click on map, selection | move | **move (unchanged)**, now on release |
| Shift+RMB / Ctrl+RMB | checkpoints / engage | unchanged |
| RMB **hold ≥ 250 ms** (≤ 4 px drift) | — | context menu at cursor |
| RMB on a plan CP, leg, own unit, contact or graphic | move | context menu for that object |
| RMB with nothing selected | nothing | context menu |
| RMB while a tool is active | cancel tool | unchanged |

Option `right_click = move | menu` in settings (default `move`); with `menu`, a click opens the
menu with MOVE HERE first and focused, so RMB, Enter moves. `Input` needs `right_down`,
`right_released` (raylib `is_mouse_button_down/released`); acting on release adds no perceptible
latency.

Option `pause_on_menu` (default on): opening the context menu or editing a plan pauses the
clock; closing restores the previous state (it is real-time-with-pause; this is the WEGO feel of
TacOps on demand).

### 4.2 Context menu widget (`hud.rs`)
Linear cascading menu, not radial: the entries need values and submenus, and must be usable from
the keyboard. Monospace uppercase rows: `LABEL ......... VALUE  [K]`, `▸` for submenu, a check
mark for the current value, greyed rows with a tooltip saying why (NO BATTERY, NO SMOKE ROUNDS).
Keyboard: up/down, right/Enter open, left/Esc back, letter accelerators. Clamped to the screen,
never over the command card's hit area. Build rows with a `Game::context_menu_buttons()` that
both drawing and hit-testing use (CLAUDE.md gotcha), dispatching existing `GameAction`/`Cmd`
values plus new `PlanEdit(..)` actions, so a menu item and its hotkey do the same thing.

Menus by target (first item is the default):
- **Ground**: MOVE HERE ▸ speed · ADD CP TO PLAN · BATTLE POSITION HERE (then facing click) ·
  SMOKE ROUNDS HERE · CALL FOR FIRE ▸ HE / SMOKE · DRAW ▸ PL / OBJ / TRP · LOS FROM HERE ·
  VIEWSHED HERE · NEXT ORDER ▸ NOW / ON ENEMY CROSSING / IN 5 MIN (the existing contingency).
- **Plan CP**: LEG TO CP ▸ SPEED / FORMATION / TECHNIQUE / WEAPONS / ON CONTACT · AT CP ▸
  FACE… / OCCUPY BP… / SMOKE SCREEN / FIRE SMOKE AT… / CALL FOR FIRE AT… / REPORT · WAIT ▸
  CODE WORD / TIME / FOR N MIN / ENEMY CROSSES PL… / PLT REACHES CP… / NONE · ON RELEASE ▸
  (same actions as AT CP) · IF ON LEG ▸
  CONTACT / UNDER FIRE / STRENGTH ≤ N → GO TO CP… · INSERT CP BEFORE · DELETE CP ·
  START HERE (skip) · CLEAR PLAN. Items ending in `…` enter a targeting tool (click a point or
  bearing on the map; right-click cancels), reusing the existing `Tool` variants.
- **Plan leg** (the line between CPs): INSERT CP HERE · leg settings as above.
- **Own unit/platoon**: SELECT · SHOW PLAN · CLEAR PLAN · ON CONTACT ▸ · quick orders (HALT,
  BREAK CONTACT, SMOKE).
- **Contact**: ENGAGE · CALL FOR FIRE ON · SMOKE ROUNDS ON · LOS TO.
- **Graphic** (PL/OBJ/TRP): CONTINGENCY ▸ ON ENEMY CROSSING / ON PLT CROSSING · DELETE.

### 4.3 Plan editing and display
- Selecting a platoon with a plan draws it: numbered CP squares, legs with dash pattern by speed
  setting (not colour only), a short label at mid-leg at sufficient zoom (`25 KM/H · WEDGE ·
  TRV OW`), badges at the CP (wait glyph + `HAMMER`, BP symbol with facing, smoke, fire), a
  dotted arrow from a CP to its branch target labelled `IF CONTACT`.
- **ETA** next to each CP (TacOps): cumulative time from now using the client's pathfinder on the
  planning speed raster and the leg's speed ceiling (formation speed for MAX); after a wait with
  no time bound show `+?`.
- Left-drag a CP to move it; Delete removes the selected CP; RMB on a leg inserts.
- The right panel shows the plan as a step list while a CP is selected (clickable rows = the same
  entries as the CP menu, values shown), so a plan can be reviewed without opening menus.
- Edits go to a client **draft**; drawn dashed until SEND (Enter or the sub-card button), solid
  once acknowledged (existing convention). REVERT drops the draft. The command card shows a
  plan sub-card while a draft exists: SEND, REVERT, DELETE CP, PREV/NEXT CP.
- Building a plan from scratch: ROUTE tool and Shift+RMB still make a plain route; a new card
  button PLAN (hotkey P) starts a draft from the platoon's current route (or empty) and CPs are
  added with left clicks, then edited with the menu.

### 4.4 Code words (synchronised execution)
A code word is the synchronisation tool: several platoons move to their CPs, each waits on the
same word, and one press releases all of them at once, each running its own `on_release` actions
(smoke, call for fire, move) and next leg. Readiness comes from the platoons' own reports, not
truth: a platoon counts as READY once it has reported `AtCheckpoint { waiting: true }` (and its
BP, if any, is set).
- A code-word strip on the command card (or top bar) lists the words used by sent plans, with
  hotkeys Ctrl+1…9 and the platoons waiting on each and their readiness (`HAMMER  1 PLT SET · 3 PLT MOVING`, `HAMMER  ALL SET`); an
  optional auto-pause option "all set on a code word" stops the clock when the last one reports. Pressing one sends
  `CodeWord`; the radio shows a terse broadcast (follow the existing brevity style in
  `radio_text`).

### 4.5 Radio text
New reports in `radio_text`: at checkpoint (`<CS>: CP 3. WAITING HAMMER.`), branch
(`<CS>: CONTACT, MOVING TO CP 2.`), plan complete, code word sent. Terse, no flavour.

## 5. Phases (each one commit or a few, each leaves `just check` green)

**P0 — Input groundwork (client, small).** `Input` gets `right_down`, `right_released`; plain
right-click move acts on release; no other behaviour change.
*Done when:* existing CLI scripts and screenshots behave as before.

**P1 — Plan core (sim).** Data types, `SetPlan`, runner with leg settings, pass-through CPs,
arrival, `Face`/`Report` actions, `wait` with `AtTime`/`Elapsed`, plan view, reports,
`state_hash`, §3.4 invariants. Tests in `crates/sim/tests/plans.rs`:
1. Two legs: MAX in column to CP 1, then 15 km/h wedge to CP 2 → no member exceeds 15 km/h
   (+ tolerance) after CP 1, formation is wedge within 20 s of CP 1, arrives in time T.
2. Pass-through CP keeps moving (no member's speed drops below X at CP 1).
3. Wait `Elapsed 120 s` at CP 1 → stationary for ≥ 115 s, moving within 3 s after.
4. Player `PlatoonMove` discards the plan; `Halt`/`Resume` keeps the step; a contact drill mid
   leg resumes the same step.
5. Edited plan with `start` = current id keeps progress.
6. Replay with a `SetPlan` reproduces the state hash; golden hash in `m1.rs` unchanged; RON
   round trip of `SetPlan`.

**P2 — Conditions, actions, contact reaction, code words (sim).** Extend `TriggerCondition`
(and `control::met`, `reference_sign`, `describe`), `ConditionSet`, branches, all
`StepAction`s, `SetContactReaction`, `CodeWord`, doctrine `code_words`, `OwnUnit.last_hit_s`
(or equivalent) for `UnderFire`. Doc-comment each behaviour with its doctrine source. Tests:
1. Wait for code word: holds 300 s, moves within 2 s of `CodeWord`. Two platoons waiting on the
   same word both depart within the same 1 Hz tick, and `on_release` (`SmokeScreen`,
   `CallForFire`) runs before the departure.
2. `Continue` under contact: never enters `Contact`; `Drill` (default) does.
3. Branch on contact during leg 2 → reaches the branch CP; smoke launched when the action list
   has `SmokeScreen`.
4. `OccupyBp` action, then wait: reports set, then continues on code word.
5. `PlatoonAt` synchronisation between two platoons.
6. Conditions use the picture only (a hidden enemy never fires `EnemyInArea`).

**P3 — Context menu (client).** Widget, RMB semantics (§4.1), ground/unit/contact/graphic
menus dispatching existing actions, options `right_click` and `pause_on_menu`, help text.
CLI for screenshots: `--rclick x,y[,hold]` (world coordinates) and menu navigation through
`--keys`. *Done when:* every existing order is reachable from the ground menu and a
`xvfb-run … --rclick … --screenshot` shows the menu; GL 2.1 clippy passes.

**P4 — Plan editing (client).** Draft, PLAN card button and sub-card, CP and leg menus,
targeting sub-tools, drag/insert/delete, map display with badges, mid-leg labels, branch arrows,
ETA, step list in the right panel, SEND/REVERT, editing a running plan. CLI: `--plan` scripting
of a simple plan (RON or `1:x,y,x,y` plus `--keys`) for screenshots and AAR checks.
*Done when:* the example in §1 can be built with the mouse alone and with the keyboard
(menus + accelerators), runs to completion on the demo map, and the replay verifies with
`tanks-headless replay`.

**P5 — Code words, radio, docs.** Code-word strip with readiness, radio text, AAR shows code words on the
timeline (if cheap), DESIGN.md §8.3 (plans, contact reaction, code words), §10.3
(right-click/menu/plan editing), §13 roadmap row for this work (before M7e) and
§13.1 status + deferred list, the status line in CLAUDE.md, §14 #22–#24, CLAUDE.md codebase
map (`plan.rs`, tests file, CLI flags), REFERENCES.md (TacOps, FM 6-0, Steel Beasts route
conditions).

Order of work: P0 → P1 → P2 can run before or in parallel with P3 (P3 only needs existing
orders) → P4 → P5. Do not start P4 before P1 lands (the client edits `ai::picture::Plan`).

## 6. Realism notes (keep these honest in DESIGN.md)
- Execution on a code word, checkpoints, phase lines and event triggers are real control
  measures (FM 6-0 "on order" / execution matrix, ADP 3-90). Real orders are mostly given as
  intent plus a few such measures, and subordinates use initiative (mission command); a long
  rigid script is less realistic than a short plan with contingencies. Keep defaults that let
  the platoon AI act on its own (contact drill on unless the player chose otherwise).
- Simplifications in v1: a code word reaches every platoon instantly and is never missed (no
  acknowledgement, no jamming); readiness is reported perfectly. Tier 2 communications effects
  (DESIGN.md §13.2) can later add a short delay and missed transmissions through the same
  `CodeWord` path.

## 7. Risks and checks
- **Golden hash drift**: defaults must reproduce today's behaviour (`ContactReaction::Drill`,
  no plan). Update the hash only for an intended change and say why in the commit.
- **Runner fighting the contact drill / BP order**: the arrival and resume rules in §3.3 are the
  likeliest bugs; tests P1.4 and P2.3/2.4 cover them.
- **RMB on release** changes the feel of the quick move slightly; keep the hold threshold in
  settings if players complain.
- **Menu clutter**: keep first-level menus at ≤ 10 rows; push detail into submenus and the
  step list.
- **Old hardware**: all drawing is plain lines/rects/text in `render.rs`; no shaders.
- **Fog of war**: the client builds menus from `PerceivedWorld` only (contacts are perceived
  contacts; plan view comes from the picture).
