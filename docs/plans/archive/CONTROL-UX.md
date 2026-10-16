# Proposal: one control model for orders, routes and plans

Status: **done** (U1–U4, 2026-10-14/15; DESIGN.md §10.2–10.3, §14 #25–26), decisions recorded in §9. This is the hand-off document for
the implementing agent.
Read `CLAUDE.md`, `docs/DESIGN.md` §8.3 and §10.2–10.3, and `docs/plans/archive/PLANS-AND-CONTEXT-MENU.md`
first. Code: `crates/client/src/game.rs` (`hotkeys`, `update`, `command`, `card`, `prompt`),
`game/ctx.rs` (right button, context menu), `game/planedit.rs` (plan draft, plan card),
`mapscreen.rs` (`help_lines`).

## 1. Summary

**Problem.** The game has two ways of telling a platoon what to do. They overlap, but nothing on
screen says which one is in use:

1. **Quick orders** (RTS style): command-card buttons, hotkeys and the plain right click. They
   take effect now and replace what the platoon was doing.
2. **The checkpoint plan** (TacOps / Steel Beasts style): a drawn route whose legs and
   checkpoints carry settings, edited as a draft and sent with SEND.

These two cover most of the same ground: moving, speed, formation, technique, weapons, contact
reaction, battle positions, smoke, fire and timing. But they differ in **scope** (now or later),
**commit** (sent at once, or held in a draft), and **persistence** (some settings are hidden
client-side "next order" defaults). A third, older layer also adds sequencing: the ROUTE tool,
Shift+right-click, and the TIMING button's QUEUE / ON PL / +5 MIN. The player has to remember
which mechanism a given key belongs to, and the screen does not help them. That is the
"confounded" feeling.

**Proposal, in one sentence.** Each platoon has **one** set of orders, and that set *is* its
checkpoint plan, drawn on the map. Every click, key and button **edits that one object**, with
one scope rule, one commit rule and visible feedback before and after each action.

Concretely:
- **One object.** A move, a route and a plan are the same thing at different sizes. Holding Shift
  means "add to the orders", and leaving it off means "replace them" (the StarCraft queue
  convention), for clicks *and* for order buttons.
- **One scope rule for posture** (formation, technique, speed, weapons, on-contact). It applies to
  **what is selected**: the platoon, meaning now, or a selected leg/CP, meaning later. Posture
  buttons show a list of values instead of cycling.
- **No hidden modes.** Remove the client-global "next move" settings (technique, move speed). Make
  TIMING one-shot. Never pause the clock without saying so.
- **Safe editing without a mode.** The selected platoon's path is always editable in place. A
  draft is made only by a real edit, is kept per platoon, is never silently thrown away, and has
  undo. The command card keeps its layout, so drafts get a strip instead of a sub-card that
  replaces the card.
- **Show before acting** (feedforward). A ghost of the order under the cursor, a hint line saying
  what each mouse button would do here, and a fill ring while the right button is held.

The sim changes are small (§7). Almost all of the work is in the client.

## 2. Today's controls (inventory from the code)

### 2.1 Mouse, no tool active
| Input | Effect | Where |
|---|---|---|
| LMB on own unit | select platoon (Ctrl: one vehicle; Shift: add/remove platoon) | `Game::select` |
| LMB drag | box-select platoons (Shift adds) | `box_select` |
| LMB on empty ground | deselect | `select` |
| LMB on a CP of the shown plan | selects the CP, **creates a draft at once and so stops the clock** (pause on menu); dragging moves the CP | `plan_mouse` → `ensure_draft` |
| RMB click on ground, with a selection | move (on release); **discards route, BP order and plan** | `rmb_update` |
| RMB on an object, nothing selected, or option RIGHT CLICK = MENU | context menu | `rmb_update` |
| RMB held 250 ms | context menu (nothing shows that a hold is in progress) | `rmb_update` |
| Shift+RMB | add a CP to a *route* draft (copied from the current route); sent when Shift is released | `update` |
| Ctrl+RMB | engage the contact, with `sel_unit` only: **with a platoon selected this is its first vehicle** | `engage_at` |

### 2.2 Inside tools
| Tool | LMB | RMB | Enter | Esc |
|---|---|---|---|---|
| ROUTE (R) | add CP | **add destination and send** | send | discard everything |
| PLAN / ADD CPS (P) | add CP | **stop adding** (draft kept) | send plan | 1st: stop adding; **2nd: revert the whole draft** |
| MOVE, ENGAGE, SMOKE RND, CALL FIRE, ADJUST, BP, OBJ, TRP, plan picks | place | cancel | — | cancel |
| PHASE LINE | add point | cancel (points lost) | finish | cancel |

The same right button means *finish and send*, *stop and keep*, or *cancel and lose*, depending on
which of two very similar tools is active.

### 2.3 Command card (24 cells) by what a press actually does
| Kind | Buttons |
|---|---|
| Needs a map click, then orders now | MOVE, ROUTE, BATTLE POS, ENGAGE, SMOKE RND, CALL FIRE, ADJUST |
| Orders now | HALT/RESUME, SMOKE, ENG SMOKE, REVERSE, BREAK CONT |
| Cycles, **one radio order per press** | FORMATION (9 values), WEAPONS, AMMO |
| Hybrid: orders the selection now **and** sets the next move | MOVE SPEED |
| Client-global setting for the *next* order only, not per platoon | TECHNIQUE, TIMING (sticky), BP radius, fire kind (Y again) |
| Staff tools | PHASE LINE / OBJECTIVE / TRP (all three on G), VIEWSHED, LOS |
| Plan | PLAN, which swaps the whole card for an 8-button plan sub-card while a draft exists |

The selection panel shows the confusion directly: `FORM WEDGE  SPEED MAX  NEXT TRAVEL, MAX`. Two
values belong to the platoon, and two are hidden client defaults for an order not given yet.

### 2.4 Overlapping ways to do the same thing
| Intent | Ways today | Results |
|---|---|---|
| Path with checkpoints | Shift+RMB · ROUTE tool · menu ROUTE VIA HERE · PLAN tool · menu ADD CP TO PLAN | two different objects (route `Move{via}` and `Plan`) that look alike on the map but edit differently |
| "Do this after that" | TIMING=QUEUE · plan CPs · Shift+RMB | three mechanisms |
| "Do this later / on a signal" | TIMING=ON PL / +5 MIN · plan WAIT · code words | two condition systems in the UI (they share `TriggerCondition` in the sim) |
| Set speed | S (now + next) · menu MOVE HERE AT ▸ (**also changes the global default**, `run_menu_act`) · plan leg SPEED | three scopes, one of them a hidden side effect |
| Formation / technique / weapons / on contact | card · unit menu ON CONTACT · plan leg menu | technique cannot be changed "now" at all (it travels only with a move) |

### 2.5 Specific defects found while reading
1. **Hidden pause.** With PAUSE ON MENU (the default) the clock stops whenever a draft exists
   (`update`: `editing && pause_on_menu`), but `state_text` still reads `RUNNING X1`.
2. **Silent loss of work.** Selecting another platoon reverts the draft (`update`:
   `plan_revert` when `sel_platoon` changes). A second Esc reverts it too. A plain RMB on the
   ground while drafting sends a quick move that cancels the running plan, and leaves the draft
   open.
3. **Clicking a CP to look at it creates a draft**, which pauses the clock and swaps the card.
4. **TIMING is sticky.** After one ON PL RED order, every later order is a contingency until
   the player cycles TIMING back to NOW.
5. **Cycling sends orders.** Getting from WEDGE to ECHELON R takes several presses of F, and each
   press is a radio order with its own delay and radio line.
6. **The plan sub-card hides the normal card,** so HALT, SMOKE or BREAK CONTACT cannot be
   reached from the card while a plan is being edited, even though those are emergency orders.
7. **Hotkeys.** One key is shared by three buttons (G), C works only in plan mode, and the
   mnemonics mix RTS habits (A = engage) with arbitrary free letters (Y, J, N, Q, D, K).
8. **The hold gesture has no signifier.** Nothing shows that holding RMB does something
   different from clicking it.

## 3. Principles (literature) and how they apply

| Principle | Source | Applied here |
|---|---|---|
| **One conceptual model.** The user's mental model has to match the system's model, and the screen has to communicate it. Two overlapping models produce "gulfs of execution and evaluation". | Norman, *The Design of Everyday Things* (rev. 2013), ch. 1–2 | One object per platoon (its orders = plan). Every control is an editor of that object |
| **Avoid modes; prefer quasimodes.** A mode the user must remember causes mode errors. A *quasimode*, held by a key, cannot be forgotten. | Raskin, *The Humane Interface* (2000), ch. 3; Tesler, "Don't mode me in" | Shift = append is a quasimode (keep it). TIMING, TECHNIQUE and MOVE SPEED as sticky "next order" state are modes (remove them). The plan sub-card is a mode (replace it) |
| **Visibility of system status; user control and freedom (undo); consistency; error prevention; recognition rather than recall** | Nielsen, *10 Usability Heuristics* (1994, NN/g) | Show PAUSED (EDITING). Undo for drafts and for a replaced plan. One meaning per mouse button. Lists of values instead of cycling |
| **Direct manipulation**: visible objects, incremental reversible actions, immediate feedback; **permit easy reversal** | Shneiderman, "Direct Manipulation" (1983); *Eight Golden Rules* | Drag CPs and legs on the map. Every edit is reversible. No hidden edit state |
| **Feedforward**: show what an action *will* do before it is taken | Vermeulen et al., "Crossing the Bridge over Norman's Gulf of Execution: Revealing Feedforward's True Identity", CHI 2013 | A ghost order under the cursor, with "REPLACES PLAN" when that applies, plus mouse-button hints |
| **Novice → expert transition**: the same action reachable by a visible path (menu, button) and a fast path (hotkey, gesture), where using the slow path teaches the fast one | Kurtenbach & Buxton, marking menus (CHI 1993/1994); Cockburn, Gutwin, Scarr & Malacria, "Supporting Novice to Expert Transitions in User Interfaces", ACM Comput. Surv. 2014 | Keep the hotkey on every button and menu row. Add a fill ring to the press-and-hold menu (the marking-menu delay) |
| **Spatial stability**: interfaces that keep items in fixed places let users build spatial memory | Cockburn et al. 2014; Scarr et al., "Dips and ceilings", CHI 2011 | The command card never swaps out its whole layout for a long-lived state |
| **Perpetual intermediates**: most users learn the basics quickly, never read the manual and never become experts | Cooper, *About Face* (4th ed., 2014) | Optimise for "I know what I want; show me how". Keep the context menu, hint line and tooltips |
| **Hick–Hyman / Fitts** | Hick 1952; Fitts 1954 | Group the 24 cells into labelled rows. Short lists (≤ 10). Emergency orders in fixed, large cells |

Game precedents (interaction, not content):
- **StarCraft II.** Right click is the "smart" contextual command. Shift queues *any* order
  (move, attack, build) as a waypoint. The command card is a fixed grid with an optional
  grid-position hotkey layout.
- **Combat Mission (CMx2).** Each movement order lays down waypoints. Clicking a waypoint selects
  it, and the same order buttons then change *that leg's* movement mode, or add a pause, a facing
  or a target arc at that waypoint. In other words, the order buttons apply to the selected leg.
  This is the scope rule in §5.3.
- **TacOps.** Orders are attached to waypoints, with ETA labels. This game already has that.
- **Steel Beasts.** Each waypoint carries its own tactics, and routes are conditional. This game
  already has that.
- **Blender.** The status bar always shows what LMB / MMB / RMB will do in the current context.
  This is the hint line in §5.6.

## 4. Design goals (testable)
1. A player who has never read the help can give a move, a two-leg route with a speed change on
   the second leg, and a hold-until-code-word, using only what the screen shows them.
2. Every mouse button and modifier has **one** meaning across tools (§5.2).
3. No input silently discards placed points, a draft or a running plan. Anything discarded can
   be brought back with Ctrl+Z.
4. The clock never stops without the top bar saying why.
5. Every posture setting has exactly one control, and its scope is printed next to it.

## 5. Proposed model

### 5.1 One object: the platoon's orders
- A platoon's orders are a **plan** (`ai::picture::Plan`): a list of CPs with leg settings,
  actions, waits and branches. A single move is a one-CP plan. A route is a plan of plain
  pass-through CPs. A battle position is a CP with an OCCUPY BP action.
- The map always draws the selected platoon's orders in the same style: numbered CPs, a
  destination marker, leg labels when zoomed in, and ETAs. Draft, sent and running keep today's
  dash conventions.
- **Replace vs append** is the only distinction between "quick" and "planned":
  - no modifier = **replace** the orders (a new plan from here);
  - **Shift** = **append** to the end of the orders (a new CP, or a new action at the last CP).
- Wire format (decided, §9 #2). The client sends `SetPlan` (`start` = the current CP) whenever
  the player edits an existing plan or sets anything on a CP or leg. Otherwise it sends the plain
  orders: `PlatoonMove` for a move, `PlatoonMove{via}` for a route of plain CPs, `OccupyPosition`
  for a BP. The player never sees which one was sent: the map, the step list and the radio text
  look the same either way.

### 5.2 Mouse grammar (one meaning per button)
| Input | Meaning everywhere |
|---|---|
| **LMB** | select / place / drag. Never issues an order by itself outside a targeting tool |
| LMB on a CP / leg | select it (no draft yet). Drag past 4 px moves it or, from the leg's mid-handle, inserts a CP. **The drag creates the draft** |
| **RMB click** (ground, platoon selected) | **replace** orders: move here |
| **Shift+RMB** | **append** a CP to the orders; sent when Shift is released (unchanged quasimode) |
| RMB on a contact | contact menu (ENGAGE first). Ctrl+RMB stays as the expert path |
| RMB on own unit / CP / leg / graphic | that object's menu (unchanged) |
| **RMB hold** | context menu at the cursor; a ring fills around the cursor during the 250 ms |
| **RMB in a targeting tool** | cancel the pending click **without discarding points already placed** |
| **Esc** | step back one level: close menu → cancel the pending click → stop adding points → deselect. **Never discards a draft** |
| **Enter** | commit: send the draft, finish the line, or EXECUTE in the planning phase |
| **Ctrl+Z / Ctrl+Y** | undo / redo edits to the draft, and restore the plan a replace order discarded (as a draft, not re-sent) |

Rule of thumb printed in the help: *"Right click gives orders. Shift adds to them. Left click
picks and drags. Esc backs out. Enter sends."*

### 5.3 Posture: one control, scope = what is selected
Posture = FORMATION, TECHNIQUE, SPEED, WEAPONS, ON CONTACT (and AMMO for vehicles).
- With **a platoon selected and no CP selected**, a posture control orders the platoon **now**.
  It also updates the leg being driven, so the plan does not override it at the next CP. This
  needs `SetTechnique` (§7).
- With **a CP or leg selected**, the same control sets **that leg's** setting in the draft (as
  Combat Mission does). The leg menu entries stay, as a second path to the same edit.
- The card's posture row has a header naming the scope: `POSTURE · 1 PLT · NOW` or
  `POSTURE · LEG TO CP 3 (DRAFT)`. Values show what the target *has*, not a "next" default.
- Pressing a posture button (or its hotkey) opens a **short list** of values at the button. It
  reuses `PopupMenu`; the key again moves down, Enter or a click picks, and digits pick directly.
  One pick = one radio order. This removes cycling (§2.5 #5).
- Delete the client globals `technique` and `move_speed`, and the `NEXT …` line in the
  selection panel. A new move inherits the platoon's current posture. MOVE HERE AT ▸ sets that
  one move's leg speed and nothing global.

### 5.4 Editing without a mode, drafts that are safe
- **No PLAN tool needed to edit.** The selected platoon's path always has handles: CP squares
  and leg mid-handles. Editing by drag, menu or posture-with-CP-selected starts a **draft for
  that platoon**.
- Adding CPs: Shift+RMB (fast path). The ADD CPS button (P) stays for mouse-only players: LMB
  adds CPs, and RMB / Esc / Enter end adding. The same tool with Shift held does the same thing.
- **Drafts are per platoon** (`BTreeMap<PlatoonId, PlanDraft>` in the client), survive selection
  changes, and are marked in the roster (`1 PLT  DRAFT`) and on the map (`DRAFT` tag at the first
  CP).
- **Draft strip instead of a sub-card.** While the selected platoon has a draft, a one-line
  strip above the command card reads `DRAFT · 3 CHANGES   SEND (ENTER)   REVERT   UNDO (CTRL+Z)`.
  The card itself is unchanged, so HALT, SMOKE and BREAK CONTACT stay where they always are.
  The CP-specific commands (prev/next CP, delete CP, CP menu) move to the selection panel's step
  list, which already lists the CPs, and to the CP's context menu.
- REVERT asks for confirmation when the draft has more than one change.
- A **replace** order (plain RMB, B, X …) on a platoon with a draft or a running plan: the
  feedforward ghost (§5.6) warns before the click; afterwards Ctrl+Z restores the old plan *as a
  draft*. Nothing is re-sent automatically.
- **Planning phase:** orders are immediate anyway, so edits commit at once (no draft, no SEND),
  and undo still works (decided, §9 #1).

### 5.5 Timing and sequencing
- **Shift + any order button** appends that order as a step: Shift+B adds a CP with OCCUPY BP,
  and Shift+Z adds SMOKE SCREEN at the last CP. This replaces TIMING = QUEUE.
- **TIMING becomes one-shot** (decided, §9 #3). It arms a condition for the *next* order
  (ON PL RED, +5 MIN), the button is highlighted and the hint line names it while armed, and it
  resets to NOW after that one order (a multi-platoon selection counts as one order). It is not
  folded into plan waits. The QUEUE value goes (Shift+order replaces it).
- Code words, waits and branches are unchanged.

### 5.6 Feedback and feedforward
- **Top bar:** `PAUSED · EDITING` (or `· MENU`) whenever `pause_on_menu` holds the clock, in the
  same amber as PAUSED.
- **Mouse hint line** in the bottom status bar (Blender style). It is updated every frame from
  the same logic that handles the click, so it cannot be wrong. Examples:
  `RMB MOVE 1 PLT (REPLACES PLAN) · SHIFT+RMB ADD CP 4 · HOLD RMB MENU · LMB SELECT`,
  `LMB DRAG CP 2 · RMB CP 2 MENU`, `LMB PLACE BP CENTRE · RMB CANCEL · [ ] RADIUS`.
- **Ghost order:** with a platoon selected and the cursor over ground, draw a faint line from
  the platoon (no modifier) or from the last CP (Shift) to the cursor, with an ETA. If the
  click would replace a running plan, draw the plan dimmed with a `REPLACE` tag.
- **Hold ring** around the cursor while RMB is held, filling over 250 ms.
- **Cursor tag** next to the pointer in targeting tools (`BP`, `FIRE HE`, `SMOKE`), as well as
  the centred prompt.

### 5.7 Command card layout (fixed, grouped)
```
ORDERS        MOVE M   HALT H   BATTLE POS B  ENGAGE A   REVERSE D  BREAK CONT X  SMOKE Z   PLAN P
POSTURE · 1 PLT · NOW   FORMATION F  TECHNIQUE T  SPEED S  WEAPONS W  ON CONTACT C  AMMO Q  ENG SMOKE E
FIRES / STAFF SMOKE RND N  CALL FIRE Y  ADJUST J  TIMING K  GRAPHICS G ▸  VIEWSHED V  LOS L
```
- ROUTE disappears as a separate button. Shift+RMB and PLAN (ADD CPS) cover it, and menu ROUTE
  VIA HERE and ADD CP TO PLAN merge into **ADD CP** (Shift+RMB shown as its key).
- GRAPHICS opens PL / OBJ / TRP as a list, which fixes the G collision. C becomes ON CONTACT
  (the CP menu opens with RMB on the CP or from the step list). Hotkeys stay mnemonic letters
  (decided, §9 #4); U4 removes the collisions and nothing else.
- Row labels are drawn at the left of the card. The row order runs from most urgent to least.

## 6. What is removed or merged
| Today | After |
|---|---|
| ROUTE tool, Shift+RMB route, PLAN tool, menu ROUTE VIA HERE / ADD CP TO PLAN | Shift+RMB or ADD CPS → one plan |
| Plan sub-card (SEND, ADD CPS, PREV/NEXT, CP MENU, DELETE CP, REVERT) | draft strip + step list + CP menu; card unchanged |
| TECHNIQUE / MOVE SPEED as "next move" globals, `NEXT …` panel line | posture of the selection (now) or of the selected leg |
| Cycling F / W / Q / S | value lists |
| TIMING sticky, QUEUE | one-shot TIMING; Shift+order appends |
| MOVE HERE AT changing the global speed | sets that move's speed only |
| Esc reverting a draft; selection change reverting a draft | never; REVERT (confirmed) or Ctrl+Z |
| Click on CP creating a draft | only an edit creates a draft |
| Option RIGHT CLICK = MOVE \| MENU (`right_click` in settings, `Game::rclick_menu`) | removed: right click always moves on ground, the menu is the hold or a click on an object |
| A posture order kept while the next CP's explicit setting silently overrode it | the posture order rewrites the leg in progress (§7) |

## 7. Sim impact (small)
- `Command::SetTechnique { platoon, technique }`: posture "now" for technique (today it only
  travels with `PlatoonMove`). It applies to the move in progress, like `SetSpeed`. It needs
  replay round-trip and hash coverage. The golden hash in `m1.rs` must not change (the variant is
  new and unused by old replays).
- **Posture now, while a plan runs** (decided, §9 #5: what you set is what you get). Today a
  posture order (`SetSpeed`, `SetFormation`, `SetTechnique`, `WeaponsControl`,
  `SetContactReaction`) keeps the plan, and the next leg with an explicit setting overrides it
  (PLANS doc §3.4), so the order can be undone at the next pass-through CP without the player
  noticing. New rule, in the order's application (not the radio queue):
  - The order also writes its value into the plan's **leg in progress**: every step from the
    cursor up to and including the next *stop* (the CP the current `PlatoonMove` ends at), since
    pass-through CPs in between are applied as the leader passes them (`plan.rs`, `apply_leg`).
  - When the platoon is stopped at a CP (acting or waiting), there is no leg in progress: the
    order applies now, and the **next** leg keeps its planned setting when the platoon departs.
    The player planned that leg deliberately, and the card says so (below).
  - Later legs are never touched.
  - Plan-issued applications (`from_plan`) do not rewrite anything.
  - The rewritten steps are part of the plan the picture shows (`PlanView`), so the client's step
    list and leg labels update by themselves, and they are covered by `state_hash`. Hash the plan
    steps if they are not hashed today.
  - Client: while a plan runs, the posture row header reads `POSTURE · 1 PLT · NOW → CP 3` (the
    stop the value lasts until; `→ END` when no later leg sets it), so the scope is visible.
  - Tests in `crates/sim/tests/plans.rs`: (1) leg 2 planned at 15 km/h, a `SetSpeed` MAX during
    leg 2 holds MAX through a pass-through CP up to the next stop, and leg 3's planned setting
    applies after that stop; (2) at a waiting CP, `SetFormation` applies now and the next leg's
    planned formation applies on release; (3) golden hash in `m1.rs` unchanged (no plans there).
- `Technique` is already platoon state in the sim (`apply_leg` sets it), so `SetTechnique` only
  needs the command and its application.
- Everything else is client-only. `SetPlan` with `start` already supports editing a running
  plan.

## 8. Phases (each leaves `just check` green; GL 2.1 clippy too)

**U1 — Feedback and safety (client only, no model change).** Top bar `PAUSED · EDITING`. Hold
ring. Mouse hint line. Click on a CP selects without drafting. Per-platoon drafts that survive
selection changes. Esc never reverts. REVERT confirmation. Draft undo/redo (snapshot stack on
`PlanDraft`). Ctrl+Z restores a plan replaced by a quick order (keep the last `PlanView` per
platoon before a replace). One-shot TIMING without QUEUE. MOVE HERE AT without the global side
effect. Remove the RIGHT CLICK option: `Settings::right_click_menu` (the loader already ignores
unknown keys, so old `right_click = …` lines are harmless; stop writing it), its OPTIONS row in
`menu.rs`, `Game::rclick_menu` and its use in `ctx.rs`/`mapscreen.rs`, and its mention in
DESIGN.md §10.3.
*Done when:* scripted screenshots show the paused-editing state, a hold ring (`--rclick x,y,0.15`)
and the hint line, and a unit test in `game` (client) shows a draft surviving a selection change
and Esc.

**U2 — Posture scope and value lists.** `SetTechnique` and the leg-in-progress rewrite (sim +
tests, §7). Posture row with scope header. Posture applies to the selected leg when a CP is selected. Value lists replace cycling.
Remove `technique`/`move_speed` globals and the `NEXT` line.
*Done when:* `--select 1 --press F` opens the list (screenshot), picking ECHELON R sends one
order (one radio line), and with a CP selected the same pick changes only that leg (step list
screenshot).

**U3 — One path model.** Shift+RMB appends to the plan, or to the route, which is promoted to a
plan when needed. ADD CPS replaces ROUTE. Shift + order button appends a step. The draft strip
replaces the plan sub-card. Menu items are merged. Ghost order with the REPLACE warning.
*Done when:* the §1 example of `PLANS-AND-CONTEXT-MENU.md` and goal 1 in §4 can be done with the
mouse alone and with the keyboard alone, and the replay verifies with `tanks-headless replay`.

**U4 — Card layout, hotkeys, help.** Grouped card with row labels. GRAPHICS list. C = ON
CONTACT. `help_lines` rewritten around the rule of thumb in §5.2. Tooltips state scope
("ORDERS 1 PLT NOW"). DESIGN.md §10.2–10.3 and the decision log (new #25: one control model)
updated. CLAUDE.md codebase map and CLI notes updated.
*Done when:* every card cell has a unique hotkey, the help fits on one screen, and DESIGN.md
matches the code.

Order: U1 → U2 → U3 → U4. U1 is valuable on its own and low risk. Ship it first and
play-test before U3.

## 9. Decisions (2026-10-14)
1. **Commit model: explicit draft** per platoon, sent with Enter/SEND, visible and safe as in
   §5.4; edits commit at once in the planning phase. (Rejected: sending every edit as its own
   `SetPlan`, which spams the net and restarts the order delay on every drag.)
2. **Wire format:** `SetPlan` whenever the player edits an existing plan or sets anything per CP
   or leg; plain `PlatoonMove` / `PlatoonMove{via}` / `OccupyPosition` otherwise (§5.1).
3. **TIMING: one-shot only.** Not folded into plan waits; QUEUE removed (§5.5).
4. **Hotkeys: mnemonic letters**, collisions removed in U4; no grid-key option.
5. **Posture during a plan: yes**, a "now" order rewrites the leg in progress (§7).
6. **RIGHT CLICK = MENU option: removed** (U1). Fewer options means fewer conflicting cases to
   design, test and explain later; the hint line and hold ring teach the menu instead.

Record #1, #2 and #5 as DESIGN.md §14 #25 (one control model: orders = plan, Shift appends,
drafts per platoon) and #26 (posture rewrites the leg in progress) in U4.

## 10. Risks and checks
- **Shift release sends.** Releasing Shift to pan sends the appended CPs early. This is accepted
  (it is the quasimode). The ghost line and the hint line make it visible, and Ctrl+Z restores
  the previous plan.
- **Spatial memory.** Moving buttons once (U4) costs existing habits. Do it in one commit and
  keep the hotkeys that do not collide.
- **Fog of war.** Ghost ETAs and hints use the client pathfinder and `PerceivedWorld` only (rule
  3), as the plan ETAs do today.
- **Old hardware.** All new drawing (ring, ghost, strip) is lines, rects and text in `render.rs`.
- **Determinism.** Client undo only re-sends orders through `Command`. Nothing changes in the
  sim except `SetTechnique` and the leg-in-progress rewrite (§7).

## 11. References
- D. Norman, *The Design of Everyday Things*, revised ed., Basic Books, 2013.
- J. Raskin, *The Humane Interface*, Addison-Wesley, 2000 (modes, quasimodes, habituation).
- L. Tesler, "A Personal History of Modeless Text Editing and Cut/Copy-Paste", *interactions* 19(4), 2012.
- J. Nielsen, "10 Usability Heuristics for User Interface Design", Nielsen Norman Group, 1994 (rev. 2020).
- B. Shneiderman, "Direct Manipulation: A Step Beyond Programming Languages", *IEEE Computer* 16(8), 1983; *Designing the User Interface* (Eight Golden Rules).
- J. Vermeulen, K. Luyten, E. van den Hoven, K. Coninx, "Crossing the Bridge over Norman's Gulf of Execution: Revealing Feedforward's True Identity", CHI 2013.
- G. Kurtenbach, W. Buxton, "The Limits of Expert Performance Using Hierarchic Marking Menus", CHI 1993; "User Learning and Performance with Marking Menus", CHI 1994.
- A. Cockburn, C. Gutwin, J. Scarr, S. Malacria, "Supporting Novice to Expert Transitions in User Interfaces", *ACM Computing Surveys* 47(2), 2014.
- J. Scarr, A. Cockburn, C. Gutwin, P. Quinn, "Dips and Ceilings: Understanding and Supporting Transitions to Expertise in User Interfaces", CHI 2011.
- A. Cooper et al., *About Face: The Essentials of Interaction Design*, 4th ed., Wiley, 2014 (perpetual intermediates).
- W. E. Hick, "On the rate of gain of information", *Q. J. Exp. Psychol.* 4, 1952; P. M. Fitts, "The information capacity of the human motor system…", *J. Exp. Psychol.* 47, 1954.
- Game interaction precedents: StarCraft II (smart command, Shift queue, grid hotkeys); Combat Mission CMx2 (per-waypoint movement modes, pause/face at waypoints); TacOps (waypoint orders, ETAs); Steel Beasts Pro PE (per-waypoint tactics, conditional routes); Blender status-bar mouse hints.
