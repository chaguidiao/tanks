<div class="part"><div class="pn">PART III</div><div class="pt">Strategy and doctrine</div>
<p>How tank battles are won, checklists for defending, attacking and delaying, what changes with the era, the weather and the time of day, hints for every scenario, and a primer on the military doctrine the game is built on.</p></div>

# Principles of tank combat

Tank fights at this level are decided by a handful of ideas. Soldiers have learned them the hard way, and the simulation models the physics and procedures behind every one.

## 1. See first, shoot first

The side that detects first gets the first shot, and first shots kill. A tank that is moving, firing or silhouetted on a skyline is spotted several times faster than one that is still and hidden. **So:** stay still and hidden while the enemy moves; move when the enemy can't see you; and don't fire before your whole platoon can fire, because the first shot gives you away.

## 2. Terrain is protection

Armour can be defeated; a hill can't be shot through. Folds in the ground, forest edges and villages are your best armour. Every position should answer two questions: *what can I see from here?* and *what can see me?* Use the **LOS** and **VIEWSHED** tools to answer them during planning.

## 3. Fight hull-down

A tank in a hull-down position shows only its turret: a small target, and the best-armoured part of the vehicle. The same tank moving in the open shows everything, including its thin sides. Defending from hull-down positions against an enemy who must cross open ground is the strongest thing a tank platoon can do.

## 4. One moves, one watches

Never move everything at once when contact is possible. One section (or platoon) halts in a good spot and *overwatches*: it watches the ground ahead, ready to fire. The other moves. Then they swap. This is **bounding overwatch**. It is slow, but when the enemy fires at the moving element, the overwatch fires back immediately.

## 5. Concentrate fire, distribute targets

Four tanks firing at one enemy at a time win; four tanks firing at four enemies at once also win. Four tanks firing *at the same enemy* waste three rounds. Your platoons distribute targets on their own, but you decide where the platoons point. Put several platoons where their fields of fire overlap on the ground the enemy must cross: an **engagement area**.

## 6. Hit the flank

The front of a modern tank is very hard to penetrate; the sides and rear are not. An enemy you engage from the side, or that drives across your front, is far easier to kill. Place positions so that the enemy's approach exposes its flank to at least one of your platoons.

## 7. Use the conditions

Night, fog, dust and smoke cut sight lines, but not equally for everyone. A side with thermal imagers sees at night almost as well as by day; a side with active-infrared night sights sees about 900 m. Fog blinds thermals too. Know who has which sights (the database shows them by year) and choose your fight accordingly.

## 8. Keep something back

A **reserve** is a force you don't commit at the start, so you can react to what the enemy does. Even one platoon held back can block a breakthrough or finish a counter-attack.

## 9. Plan, then let them work

Every order you give during the fight costs 5 to 20 seconds of radio time. A plan made before H-hour, with a code word or a phase-line trigger for the critical moment, executes instantly. Good commanders spend most of their effort in the planning phase.

## 10. Watch the count

Each side is beaten at 70 % losses, and the enemy's attack usually breaks off well before that. Trading one of your tanks for one of theirs is often a losing deal when they have twice as many. Preserve your force: break contact rather than fight a losing exchange.

# Defending

Most scenarios start with you defending. A good defence is a trap: the enemy drives into an area your guns cover from protected positions.

## A defence checklist

1. **Find the enemy's approaches** (*avenues of approach*). Where can a tank company actually drive? Roads, open fields and valleys. Forests, steep slopes, rivers and villages channel it. Use the **GO/NO-GO** layer.
2. **Choose the engagement area**: the open ground on that approach where you want to destroy the enemy, ideally 1.5–2.5 km from your positions (inside your effective range, at the edge of his).
3. **Choose battle positions** around the engagement area so that each platoon covers it, preferably from different angles (flank shots), with cover behind each for moving out. Put each BP just *behind* a crest or a forest edge, not on top of it (a tank on the crest is silhouetted against the sky).
4. **Check with the tools.** Viewshed from each BP: does it see the engagement area? Viewshed from the enemy's approach: is the BP hidden?
5. **Draw a phase line** across the approach at the far edge of the engagement area. Reports will tell you where the enemy is relative to it, and you can trigger on it.
6. **Plan the next position.** Where does each platoon go if it is pressed? Give it a contingency (TIMING ▸ ON CROSSING a nearer phase line) or a plan branch (IF STRENGTH <= 2 ▸ GO TO CP …).
7. **Keep a reserve** if you have three platoons: hidden, central, with routes to each BP.
8. **Artillery:** plan fire on the approach where the enemy will bunch up (bridges, gaps between woods). Call it early: the shells arrive about two minutes after you ask.
9. **Weapons:** FREE is right for most defences. For an ambush, set HOLD or TIGHT so the platoon doesn't fire at the first recon vehicle and give away the trap.
10. **Execute**, speed up the clock, and drop back to X1 when the first contact reports arrive.

## During the defence

- **Don't move into the open to "get a better shot".** Your platoons will fire from their positions.
- **Displace before you are fixed.** When the enemy closes inside 1 km, or a platoon loses a vehicle, move it to its next position, under smoke and by reverse if needed (**REVERSE**, **BREAK CONT**). A tank that stays too long in one place gets found by artillery.
- **Watch the flanks.** If contacts appear on one side only, the enemy may be going round you.

# Attacking

Attacking is much harder than defending. The defender is hidden and waiting; you have to cross ground in view. Expect to take losses, and make every move count.

## An attack checklist

1. **Find the enemy first.** Look at the briefing and the map: where would *you* defend? Send a platoon (or your reconnaissance vehicles) forward slowly in bounding overwatch to make contact, while the rest stay back in cover.
2. **Plan a covered route** to a position from which you can fire on the objective: an **attack-by-fire** or **support-by-fire** position, hull-down, 1.5–2.5 km away.
3. **Split your force**: one or two platoons as the **support** element (they occupy the support-by-fire position and shoot at everything they can see), one as the **assault** element (it moves onto the objective while the defenders are busy or blinded).
4. **Blind the defender before you move**: artillery HE and smoke on the objective, tank smoke rounds on known positions.
5. **Synchronise with a code word.** Give each platoon a plan whose last waiting checkpoint is its start point, waiting on HAMMER. When the code-word strip shows `ALL SET`, fire the smoke, then send HAMMER.
6. **Assault fast and in line** with ON CONTACT set to **CONTINUE**: halting in the open to fight is the worst thing an assault can do.
7. **Consolidate**: once on the objective, occupy battle positions facing where the enemy's counter-attack will come from. The briefings often warn you about a reserve.

::fig g-plan-hammer | crop=560,30,600,520 | w=60 | A synchronised advance: smoke on release of HAMMER, then a line formation at 25 km/h to the next checkpoint.

## Using the example plan as a template

The plan shipped as `docs/plans/example-plan.ron` shows the pattern for one platoon: column at full speed to a start point (CP 1); wedge, traveling overwatch, 15 km/h, CONTINUE on contact to a hull-down BP at CP 2; wait for HAMMER; on HAMMER pop smoke and advance in line at 25 km/h to CP 3; *if contact within 2 km on that leg*, branch back to CP 4 behind CP 2, smoke and re-occupy. Copy the idea, not the coordinates.

# Delaying

A **delay** trades ground for time: you fight from a forward position, inflict losses, and fall back to the next position before the enemy can fix you there, repeating until the time runs out. *Night Delay* and *Delay in Depth* are delays.

- Fight at **long range**, where your sights and guns are better than the enemy's.
- Prepare **two or three lines of positions**, each covering the next one's withdrawal route.
- **Decide the trigger in advance** (a phase line, a contact range, a loss) and pre-plan the move with a contingency or a plan branch. The scenario notes say it plainly: *move on the trigger, not when the first vehicle is lost*.
- Withdraw **under smoke** and, where the ground allows, **in reverse** for the first hundred metres so the front armour stays to the enemy.
- Expect artillery on positions you have fired from.

# Era, light and weather

## Cold War (1985–1990)

- **Blue** (BULWARK A0/A1, WARD M2) has **thermal imagers** from 1980: it sees at night, through dust and through ordinary smoke. The A0 carries a 105 mm gun; the A1 a 120 mm gun.
- **Red** (KURGAN-A/B, SKIF M1) fights at night with **active infrared** sights of about 900 m range. Its 125 mm gun is powerful. From 1988 the KURGAN-B carries first-generation **reactive armour**, which removes roughly 60 % of a shaped charge's penetration (HEAT rounds and missiles) but adds little against sabot. The 1986–87 scenarios are before that; the 1990 ones are after.
- At 2 km the BULWARK A0 and the KURGAN-A can each penetrate the other's front, so terrain and the first shot decide. By 1990 the BULWARK A1 with its later sabot penetrates the KURGANs' fronts, while their sabot doesn't penetrate its front.
- The **WARD M2** carries **TOW missiles**: deadly against tanks out to almost 4 km, but slow to fly (watch them in the AAR) and fired from a thinly armoured vehicle. Keep it behind the tanks until the enemy commits.

## Modern (2005)

- **BULWARK A2** has second-generation thermals and an independent thermal viewer for the commander, and very strong frontal armour. **KURGAN-C** (T-90A-like) has heavy reactive armour that degrades even sabot rounds, a thermal sight, and **gun-launched missiles that reach 5 km**.
- Both sides see at night. Fights start at long range; open ground is deadly for whoever moves.
- **Aim for the sides of the KURGAN-C.** Its front is very hard to defeat.

## Light

The scenario date, time and latitude give the sun's elevation, and the moon's phase adds a little light at night. Day optics lose effectiveness quickly after sunset; image intensifiers and active infrared are short-ranged; thermals are almost unaffected by darkness, but dip during **thermal crossover** for an hour or so after sunrise and before sunset, when targets and background are the same temperature.

## Weather

| Preset | Effect |
|---|---|
| **CLEAR** | Long visibility |
| **HAZE** | Visibility a few kilometres. Thermals see through it |
| **OVERCAST** | Less light: dusk and night darker |
| **RAIN** | Shorter visibility, and **soft going**: traction ×0.75, rolling resistance ×1.3. Vehicles are slower than planned |
| **FOG** | Visibility of a few hundred metres, **for thermals too**. Fights happen at short range, where armour matters less and flank shots more |
| **DUST** | Blocks eyes and day optics; thermals see through it. Fast-moving vehicles raise dust plumes in dry conditions, which makes them easier to spot by eye |

The top bar always shows the light, the preset and the visibility.

# Scenario hints

| Scenario | Year | Situation | Hints |
|---|---|---|---|
| **HOLD THE VILLAGE** | 1990 | Two tank platoons hold a crossroads village against a tank company from the north-east | The tutorial ({{ch:Your first battle}}). Battle positions either side of the village facing north-east, a phase line, execute |
| **M1 - MOVEMENT** | 1990 | One platoon, no enemy to speak of | Practise routes, checkpoints, formations and the GO/NO-GO layer |
| **MIXED FORCE** | 1990 | As Hold the Village, but Red has fighting vehicles too | Red tanks have reactive armour. Crews choose sabot against tanks and HEAT against IFVs; leave AMMO on AUTO |
| **FOG AT DAWN** (1990) | 1990 | Defend in dense fog at first light | Contact at 250 m. Set up overlapping fields of fire close in; expect flank shots |
| **NIGHT ATTACK** | 1990 | Defend at 02:00 under a half moon | Your thermals see much further than Red's active IR. Engage at long range |
| **FIRE SUPPORT** | 1990 | Hold the village with a 155 mm battery | Call HE on the Red axis *early*: the FDC needs about a minute and the shells another 20–40 s. Smoke rounds screen a position |
| **HASTY ATTACK** | 1986 | Two M1 platoons and a Bradley platoon seize a village held by tanks and IFVs, with a Red reserve behind | Bound from cover to cover; the T-72A can kill an M1 from the front at 2 km. Smoke and artillery on the village before the assault; consolidate against the counter-attack |
| **NIGHT DELAY** | 1987 | Delay from forward positions at night, then hold the village; Bradleys in reserve | Engage at ranges Red's active IR can't answer, break contact before Red closes to 1 km. Expect artillery on your first positions. Commit the reserve when the village is contested |
| **VILLAGE DEFENCE** | 1987 | Hold a village in wooded hills against 25 Red vehicles | The forest cuts sight lines to a few hundred metres. Hull-down positions on the forest edge, keep the TOW vehicles behind the tanks until Red deploys |
| **DELAY IN DEPTH** | 2005 | Delay with M1A2s against an advance guard led by reconnaissance, then hold the village; M2A2 reserve | Don't let the forward platoons be fixed; move on the trigger. Forested slopes are your best cover |
| **DUSK ATTACK** | 2005 | Seize a village on open farmland held by T-90As and T-72Bs with missiles to 5 km | No cover on the approach. Use smoke to break the missile engagement, move in bounds, aim for the sides of the T-90A |
| **FOG MEETING** (2005) | 2005 | Hold the crossroads in fog with an M1A2 platoon and an M2A2 platoon | Thermals don't see through fog. Overlapping fields of fire; the T-90A front is hard, so aim for its sides |
| **CLEARING THE VILLAGE** | 1987 | Clear a village held by RPG squads and a missile team | Dismount the Bradleys short of the village and send the squads in first (OCCUPY BUILDINGS FACING...). Bring the tanks in behind them; alone, they drive into the rockets |
| **THE MARKET TOWN** | 1987 | Hold the north edge of a town against a motor-rifle attack through the woods | Squads in the edge blocks see the approach; tanks in the streets don't. Keep the tanks on the flanks of the town where they see the wood line, and expect the BMP squads to come in on foot

# A primer on armoured doctrine

**Doctrine** is the army's agreed way of doing things: how units are organised, how they move, how they fight, and what words they use. It lets a platoon leader who receives a three-line order know exactly what is expected. The computer-controlled leaders and crews in *Tanks* follow doctrine taken from public US Army manuals for the Blue side and from public descriptions of Soviet-style tactics for the Red side. This chapter explains the parts you will meet.

## Organisation

| Level | Size | Led by | In the game |
|---|---|---|---|
| **Crew** | 3–4 soldiers in one vehicle: commander, gunner, driver, and (Western tanks) a loader. Red tanks have an autoloader instead | Tank commander (TC) | Each vehicle's AI |
| **Section** | 2 tanks: a leader and a **wingman** | Platoon leader or platoon sergeant | Used for bounding |
| **Platoon** | 4 tanks (Blue) or 3 (Red, and IFV platoons) | **Platoon leader (PL)** in tank 1; **platoon sergeant (PSG)** in tank 4 | The unit you order |
| **Company** | 2–3 platoons plus a headquarters section, about 10–14 tanks | **Company commander** | You |

Callsigns follow the structure: `1-2` is the second vehicle of 1st Platoon; in a Western platoon tank 1 (PL) leads with tank 2 as its wingman, and tank 4 (PSG) leads the second section with tank 3. Red vehicles use three-digit numbers: `212` is the second vehicle of platoon 21.

## Formations and techniques

The formations and movement techniques of {{ch:Posture}} come from the tank platoon manuals (ATP 3-20.15, FM 17-15). The doctrine packs set the spacing: 100 m between tanks for Western doctrine, 75 m for the generic OPFOR. The OPFOR's formation set:

::formations generic-opfor

The choice is always a trade between **control** (column: easy to steer, little firepower forward), **firepower** (line: everything forward, hard to control) and **security** (wedge: something in every direction). The technique decides how much of the platoon is moving at any moment.

## Battle drills

A **battle drill** is a short, practised reaction to a common event. It is executed without an order. The ones in the game:

| Drill | Trigger | What the platoon does |
|---|---|---|
| **Action on contact / react to direct-fire contact** | A crew reports an enemy (Western doctrine halts; OPFOR keeps moving) | Halt, turn towards the threat, engage, report; resume when there is nothing to shoot for 45 s and no enemy shot for 20 s. Afterwards, sightings alone don't trigger it again for 150 s, so a distant static enemy can't freeze the platoon |
| **Self-screening** | A vehicle is engaged | Fire a smoke-grenade salvo within 2 s |
| **Break contact** | Ordered (X), or the ON CONTACT reaction | Smoke, then withdraw about 800 m away from the threat |
| **React to indirect fire** | Artillery bursts within 150 m in the last 8 s | Moving: lift the speed limit and clear the area. Stationary: button up and hold |
| **Occupy a battle position** | Ordered (B) | Search the area for hull-down spots, assign one per tank with dispersion, move in, report the result |

## Positions

| Term | Meaning |
|---|---|
| **Battle position (BP)** | An area a platoon defends from, oriented on a threat direction |
| **Primary / alternate / supplementary** | The first position; a second one covering the same area, to move to when the first is found; one covering a different direction |
| **Hull-down / turret-down / hide** | How much of the tank shows over the cover ({{ch:Terrain analysis}}) |
| **Military crest** | A line just below the top of a hill on the enemy's side, from which you can see down the slope without being silhouetted against the sky. The top itself is the *topographic crest* |
| **Reverse slope** | The far side of a hill from the enemy. Positions on it are hidden until the enemy reaches the crest, at short range |
| **Engagement area (EA)** | The ground where you intend to destroy the enemy, covered by the fire of several positions |
| **Sector of fire** | The slice of ground a tank or platoon is responsible for |

## Control measures

**Control measures** are lines, points and areas drawn on the map so that everyone shares the same reference. In the game you can draw **phase lines**, **objectives** and **target reference points**; plans use **checkpoints**; and the platoons use **battle positions**. In real orders you would also meet boundaries, assembly areas, attack positions, lines of departure, axes of advance and limits of advance. All of them do the same job: they let a short order say a lot ("on HAMMER, 1st Platoon advances to OBJ A; 2nd Platoon supports by fire from BP 2; nobody crosses PL BLUE until ordered").

## Orders and mission command

Western doctrine follows **mission command**: the commander states the **intent** (purpose, key tasks, end state) and subordinates use their initiative within it. Orders are short and are prepared in advance wherever possible. Three tools from that tradition appear in the game:

- **"On order" execution by code word**: a task is prepared in detail and released by a single word (FM 6-0).
- **Contingencies and branches**: "if this happens, do that" is decided before the fight, so nobody has to improvise under fire.
- **Triggers on control measures**: "when the enemy crosses PL RED, displace" removes the radio delay from the decision.

## The opposing force

The Red side uses a **generic OPFOR doctrine** modelled on public descriptions of Soviet-style tactics (FM 100-2-1, TC 7-100.2):

- **Centralised control**: longer order delays (10–30 s), less initiative, and platoons that keep moving when they meet the enemy instead of halting.
- **Echeloned approach**: a **march** in column, a **pre-battle formation** (still columns, platoons spread) from about 4 km, and a **battle formation** (line) from about 1.5 km.
- **Synchronised assault**: the attacking platoons halt abreast about 3 km from the objective and shake out together, unless they are fired on first.
- **Reconnaissance first** in some scenarios: a small element leads and the main body is released when it makes contact.
- **Defence in depth**: defenders displace to alternate positions when the attacker comes within 1 km or they take losses, and a **reserve** counter-attacks when the objective is threatened.
- **Artillery** on fresh contacts, and smoke when the attack enters its battle phase.
- The attack **breaks off** after about two-thirds losses.

The AI reads only its own side's reported picture. It doesn't cheat.

## Where to read more

The doctrine in the game comes from public sources: **ATP 3-20.15** *Tank Platoon* (2025) and **FM 17-15** (1987/1996) for platoon tactics and drills; **ATP 3-90.1** / **FM 71-1** for the company; **FM 3-90** and **ADP 3-90** for tactics and control measures; **FM 6-0** for orders and code words; **TC 3-25.26** for map reading; **FM 100-2-1** and **TC 7-100.2** for the opposing force; **ATP 3-09.30** and **FM 6-40** for calling and adjusting artillery fire. The full list is in `docs/REFERENCES.md`.
