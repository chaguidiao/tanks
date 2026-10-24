# M8b infantry calibration notes (research, 2026-10)

Status: **notes for alignment, nothing implemented**. These notes check the M8b-a infantry
numbers (`data/combat/infantry.ron`, `data/infantry/*.ron`) against public data. They also
extrapolate era changes up to 2005, the latest scenario year. Sources are in docs/REFERENCES.md,
section "Infantry combat and detection". Values marked *derived* are our own arithmetic on those
sources; they are not quoted figures.

## 1. Lethality of aimed small-arms fire

### 1.1 What the sources say

| Source | Finding |
|---|---|
| Hitchman, ORO-T-160 (1952), Fort Belvoir tests | Prone riflemen with the M1 rifle, firing at a man-silhouette under mild stress (limited exposure, random order). Aiming error in mils does not depend on range. Experts pooled σ ≈ 4.9–5.8 in at 100 yd, i.e. **≈ 1.4–1.6 mrad per axis**. Marksmen firing alone at 300 yd **miss a type E silhouette (0.43 m²) 76 % of the time**. A bench-rested rifle misses it 4 % of the time, so the man is the error, not the rifle. |
| Hitchman (1952) | Lethal probability of a bullet hit is taken as **0.3**, and "the lethal index of the rifle bullet exceeds 30 percent". Bullet hits land on the body at random by exposed area, like fragments: aiming does not pick vulnerable parts. |
| Hitchman (1952), AORG and ORO interviews | About **80 % of effective rifle and LMG fire was under 200 yd and 90 % under 300 yd** (ETO, Korea). Korean rifle-bullet hits had a mean range of just over 100 yd. |
| Beebe & DeBakey, *Battle Casualties* (1952), p. 155 | Where the agent is known for all the killed, about **25 % of men hit by bullets were killed** (regression average 25 %, four clean samples 26 %). Bullets were more lethal per hit than fragments. |
| Beebe & DeBakey, Tables 50, 54, 55 | US First and Third Armies in Europe 1944–45: bullets caused **27–30 %** of deaths from wounds, shell fragments **58–61 %**. Fifth Army (Italy) sample: bullets 16 %, high explosive 84 %. Hospital admissions: bullets 23–27 %, shell 60–63 %. |
| Beebe & DeBakey, Table 72 (Bougainville) | Position of men when hit: standing 37 %, sitting 20 %, prone 22 %, in a pillbox or trench 21 %. Bullet hits only: 45 / 23 / 21 / 11 %. |
| Rowland (JORS 1987, 1991; *The Stress of Battle*, 2006), via Dupuy | Over 100 small-unit actions (Boer War, WWI, WWII) compared with field trials using laser-instrumented weapons (the British counterpart of MILES). Under real fire, effectiveness was **1/7 to 1/10 of field-trial values**. Dupuy says proving ground to field adds a degradation of similar size. In the field trials, **defensive rifle fire "almost invariably stopped any attack"**, unlike in history. |
| Rowland, via a wargaming site (secondary) | Defending against a 3:1 attack, an average rifleman inflicts about **0.5** casualties and a machine gun about **4** (about 8×). |
| Rounds per enemy killed (secondary) | WWII about 25,000, Korea about 50,000, Vietnam 50,000–200,000. These count all fire, most of it at unseen positions, so they cannot calibrate aimed bursts at a detected squad. |

### 1.2 Check of the current model

- **Aiming error: rifles 6 mrad.** Our hit formula `A / (2πσ²)` with σ = 2 mrad gives
  0.43 / (2π · 0.55²) = **0.23** at 274 m. Hitchman measured 0.24, so the formula is fine. A
  degradation of 7–10× in hits per round means σ × √7…√10, i.e. **5.3–6.3 mrad** (*derived*).
  6 mrad sits in that band.
- **Total gap from range to combat (halted target): about 26×** (σ² ratio 9 × area ratio
  0.43/0.15 ≈ 2.9). Rowland's 7–10× comes on top of field trials, which already use realistic
  targets, and Dupuy says range to field adds as much again. That puts a range-to-combat gap of
  roughly 50–100× within reach. **We are on the lenient side**, which is acceptable while
  firefights stay short.
- **Chance a hit takes the man out of the fight: 0.6.** Of men hit by bullets, about 25–30 % are
  killed. Most of the rest were hospital admissions (the counts leave out men who lost no time),
  so they were out of the fight. Taking 0.6 as "out of this fight per hit" therefore allows for
  hits on kit, grazes and men who fight on. **Supportable range 0.6–0.8.** Keep 0.6 and raise
  confidence from `estimate` to `low` with these citations.
- **MG versus rifle.** One M60 against one rifle in our data: (96 / 18 rounds per minute) ×
  (6 / 5)² ≈ **7.7×** casualties. This matches Rowland's 8× (secondary source).
- **Pinned squads keep firing** (DESIGN §14 #31). Rowland's field-trial result is the same
  artefact we saw, where the side that fired first always won. The change is supported.

### 1.3 Validation targets (for a later harness run)

| # | Target | Value |
|---|---|---|
| V1 | `round_hit(0.43, 2.0, 274)` (formula unit test, cite Hitchman) | 0.24 ± 0.03 |
| V2 | Rifle volley at a halted squad, 250 m: rounds per casualty | 100–250 |
| V3 | Casualties per weapon, one MG versus one rifle, sustained fire | 6–10× |
| V4 | Share of infantry losses from small arms in a scenario with artillery and tank HE | 15–35 % |
| V5 | Exchange ratio of 2005 Blue against 1990 Blue squads, same ground (after §3) | 1.4–1.9× |

### 1.4 Harness results (2026-10-24, `tanks-headless firefight`, 100 trials, flat grass, by day)

Blue rifle squad firing at a halted Red MR squad that holds fire; one weapon type at a time.

| Weapons | Range | Rounds per casualty | Casualties per weapon-minute |
|---|---|---|---|
| 5 rifles | 100 m | 30 | 0.64 |
| 5 rifles | 250 m | **246** | 0.076 |
| 5 rifles | 400 m | 671 | 0.028 |
| M60 class (1984) | 100 / 250 / 400 m | 21 / 156 / 468 | 4.83 / 0.63 / 0.21 |
| SAW (1990) | 250 m | 208 | 0.29 |

- **V1** passes: `round_hit(0.43, 2.0, 274.3)` = 0.23 (unit test in `sim-data`).
- **V2** passes, at the strict end of the band: 246 rounds per casualty at 250 m (target
  100–250). The visible fraction of a halted squad (about 0.65) is why this is higher than the
  160 the formula alone gives.
- **V3** passes: one M60 causes 7.5–8.2× the casualties of one rifle (target 6–10×). The SAW
  causes 3.9×.
- **V4** cannot be measured yet. `tanks-headless run` now prints men lost by cause (small arms,
  HE, with the carrier). But no side's AI dismounts its squads yet (TODO F11), so in
  `m8b-dismounts` all losses happen with the carrier. Measure V4 when F11 or a scenario with
  fighting dismounts exists.
- **V5** passes, at the low edge (B21–B23 done 2026-10-24). Mutual fire (`--mutual`), Blue
  rifle squad against Red MR squad, both halted at 250 m by day, 500 trials per year: the
  exchange ratio (Red men lost per Blue man lost) is 0.129 in 1990 and 0.185 in 2005, a
  **1.43×** gain (target 1.4–1.9×). At night (01:00, 150 m, 200 trials) the gain is 3.0× (0.35
  → 1.04), from the aiming laser and the newer goggles. One-way rifle fire at 250 m: 198 → 126
  rounds per casualty from 2002 to 2003 (300 trials), against 1.44× expected from 6 → 5 mrad.
- **Noise:** 100 trials give V2 anywhere from about 200 to 250 rounds per casualty. Use at least
  300 trials for comparisons.
- **Observation, not changed:** in mutual fire the Red squad wins about 7:1 in men lost. Its
  PK (7.62 mm general-purpose MG) causes about 8× a rifle's casualties, while Blue's SAW causes
  about 4×, and Red has seven men to Blue's six. This fits the V3 ratio. Revisit it if squad
  balance matters once AI uses infantry (F11).
- The Q1 decision stands: no value changed. Small-arms fire against men is on the lethal side
  of what history suggests, but inside the V2 band of this harness.

## 2. Detection of infantry

### 2.1 What the sources say

| Source | Finding |
|---|---|
| Hitchman (1952), map study, about 18,000 readings in six countries | 70 % of the ranges at which an erect man can be seen by a prone rifleman are **under 300 yd**, and 90 % under 700 yd. This limit is line of sight, which our LOS code already models. |
| Army camouflage Phase IV test plan (2010) | The main measure is the **distance at which observers have a 50 % chance of detecting** a camouflaged soldier. Woodland and desert are tested at **35–400 m** and **35–500 m**. Stationary camouflaged men are found at hundreds of metres, not kilometres. |
| STANAG 4347 / Johnson | NATO human reference target: critical dimension 0.75 m (about 1.8 × 0.5 m standing); vehicle 2.3 m. Johnson 50 % thresholds: detection 1 cycle, recognition 4, identification 6.4. |
| Commercial thermal camera spec | Detects a vehicle at 5.8 km, a human at 2 km and recognises a human at 600 m. The man to vehicle range ratio is ≈ 3, as the critical dimensions predict. |
| AN/PAS-13 weapon thermal sights (Strategypage; Defense Industry Daily) | Detection, individual / vehicle: light **800 m / 3,100 m**, medium 1,400 / 4,800 m, heavy 2,300 / 7,300 m (target labels assumed for medium and heavy). Recognition: light 550 m, medium 1,100 m. First issued to Special Forces in **1998**; redesigned in 2006; large numbers reached Iraq and Afghanistan in **2007**. |
| NVESD search models (ACQUIRE, Time-Limited Search, field of regard extensions) | ACQUIRE's 3.4 s mean time assumes the target is inside the sight's field of view. Searching a wider field of regard adds the time spent moving between fields of view. |

### 2.2 Check of the current model (harness run 2026-10: `tanks-headless detect`, flat open grass)

| M1A1 by day | 1 km | 2 km | 3 km |
|---|---|---|---|
| Halted: found within 2 min / mean time | 100 % / 2.7 s | 100 % / 5.3 s | 100 % / 15.7 s |
| Dug in: found within 2 min / mean time | 93 % / 3.0 s | 73 % / 15.6 s | 75 % / 37 s |

- **Ranges look right.**
  - Squad eyes (1.7 cycles/mrad × optical contrast 0.5) reach a 50 % chance of detecting a
    halted squad at about **650 m** in the open (*derived*). That is just above the 35–400 m
    woodland test band, as it should be for open ground.
  - Our tank thermal (3.5 cycles/mrad) gives a 50 % chance on a man (0.75 m) at about 2.6 km and
    on a vehicle at about 8 km. A heavy thermal weapon sight is quoted at 2.3 / 7.3 km, so our
    first-generation tank thermal sits in the same class (*derived*).
  - The ratio of man to vehicle detection range matches the PAS-13 and commercial figures (3–4×).
- **Times are too short.** ACQUIRE's 3.4 s per field of view is applied as if the target were
  always inside the sight's view. A 10× sight (about 6.5° field) sweeping a 60° sector should
  take roughly 9× longer (*derived*). This applies to vehicles too, so the change is model-wide.
  Re-check it against the Phase 0 detection calibration before changing anything.
- **Dug-in squads are too visible.** Hasty positions are camouflaged. Today 73 % are found within
  2 minutes at 2 km by optics alone. Most should be found when they fire (the firing cue is
  already 6×).

### 2.3 Proposed changes (pending alignment)

- **D1.** A camouflage factor on optical and night-channel cycles: dug-in about 0.5, halted
  about 0.8 on top of `optical_contrast`; thermal unchanged (*estimate*). Data plus a small
  model change; the golden hash is unaffected (no squads in the golden scenario).
- **D2.** A sector search factor for narrow-field sights (needs a field-of-view stat per sensor).
  It is model-wide, so it changes the golden hash and needs a re-run of the detection and
  balance batches. It deserves its own step.

### 2.4 D1 result (2026-10-24, `tanks-headless detect`, 200 trials, flat grass)

Implemented as `camouflage` in `data/combat/infantry.ron`: halted ×0.8, dug in ×0.5 on
optical and night-channel cycles; thermal unchanged; squads in buildings unchanged.

Share of trials with the target squad found within 2 minutes, and mean time to find it.
Before → after:

| Observer | Target | Range | Found in 2 min | Mean time |
|---|---|---|---|---|
| M1A1 by day (thermal) | dug in | 2,000 m | 83 % → 82 % | 21 → 23 s |
| T-72A by day (optics only) | dug in | 1,000 m | 93 % → 92 % | 5 → 19 s |
| T-72A by day | dug in | 2,000 m | 78 % → 46 % | 26 → 50 s |
| T-72A by day | halted | 2,000 m | 100 % → 100 % | 8 → 12 s |
| Rifle squad by day (eyes, binoculars) | dug in | 500 m | 89 % → 89 % | 3.5 → 14 s |
| T-72A at night (active IR) | dug in | 250 m | 98 % → 53 % | 27 → 49 s |

- **Thermal sights are unaffected, by design.** The M1A1 still finds most dug-in squads at
  2 km, because the head-and-shoulders signature row already makes the target small.
- **Optics and night devices find dug-in men 2–4× more slowly.** Over 2 minutes, though, most
  are still found. The cause is the search-time issue (D2): each look gets ACQUIRE's 3.4 s mean
  time as if the target were already in the field of view, so repeated looks soon add up. D1
  sets the contrast and D2 will set the search time.
- Test: `m8b::camouflage_slows_optical_detection_of_a_dug_in_squad` (T-72B, 1 km, 20 seeds:
  13.8 s against 4.7 s).

## 3. Era changes up to 2005

The squads were frozen at about 1990. Squad weapons and sensors already carry service dates, so
most of this is data. Body armour and night aiming need small model changes.

| Item | Blue (US-mirror) | Red (Soviet/Russian-mirror) | Effect in the model (*derived/estimate*) |
|---|---|---|---|
| **Rifle sights** | Red-dot M68 CCO adopted 1997, general issue to deployed units by about 2003–05; ACOG with the USMC from about 2004. ARL-TR-8594 (2018): mean hit rate 0.69 with iron sights against about 0.86 with the M68 at 100–400 m (from a search excerpt; check the PDF). A 2016 Army report, via a forum, gives 50 % hits with an Aimpoint at 300 m on a standing silhouette, against Hitchman's 24 % with iron sights at 274 m. | AK-74M with iron sights; optics only on the SVD (PSO-1). | Blue rifles **6 → 5 mrad from 2003** (about 1.4× more hits; optics help most by finding partly hidden targets). Red unchanged. Data only: a second, dated rifle row. |
| **Body armour** | PASGT vest from about 1983 stops fragments only. Interceptor with SAPI plates from 1999, general issue in theatre by early 2004 (ESAPI 2005); the plates stop 7.62 × 39 and 5.45 mm ball. OIF/OEF wounds: thorax **6 %** against **13 %** in WWII and Vietnam. Case fatality rate fell from about 19 % (WWII) and 16 % (Vietnam) to about 9–10 % (OIF/OEF), mostly from better care. These rates are recalled from Holcomb et al. (2006) and must be checked in the paper. | Fragment vests (6B2/6B3 in Afghanistan, 6B5 from 1985) issued only in part; still partial in 2005. | Chance a bullet hit takes the man out: Blue **0.6 → 0.52 from 2004** (fewer torso wounds, about 10–15 %). HE exposure × 0.85 for fragment vests (Blue from 1983, Red × 0.95 from 1985). **Needs code:** the chance and the HE factor become per squad, with dates. |
| **Night devices** | AN/PVS-7 from 1990. PVS-14 Gen III monocular with an AN/PEQ-2 infrared aiming laser around 2000–03 (dates unconfirmed in the sources). Thermal weapon sights for Special Forces from 1998, general issue only from 2007. | 1PN58 sight (early 1970s): detection 970 m, recognition 400 m under starlight (target type not given). 1PN93 for the AK-74M (1990s). | Blue goggles 1.0 → about 1.3 cycles/mrad, 300 → 350 m, from 2002. **No thermal weapon sights before 2006** (outside the scenario years). Red night sight 0.5 → 0.7 cycles/mrad, 300 → 400 m (1PN58 class, *low*). |
| **Night aiming** | An infrared laser and goggles allow aimed fire at night. | Iron sights through a night scope: slow, aimed fire. | **Gap: small arms are as accurate at night as by day** once a squad is detected. Proposal: aiming error × 1.6 at night without an aiming aid, × 1.1 with one (*estimate*). **Needs code.** |
| **Machine guns** | M249 SAW from 1985 (already in the data); the M240B replaced the M60 in 1997, in weapons squads rather than Bradley dismount teams. | PKM; Pecheneg from 2001, few in units by 2005. | No change. |
| **Engagement ranges** | Afghanistan: about half of engagements beyond 300 m (Ehrhart 2009, from returning soldiers' comments, mountain terrain). | | No change. Rifle reach stays 500 m and MGs 800–1,100 m. Do not generalise mountain ranges to the European scenario maps. |

Out of scope until scenarios go past 2005: small drones, general issue of thermal weapon sights
(2007), the M4 with MIL-STD-1913 rails as a separate row, and Russian Ratnik kit (2013 on).

**Implemented 2026-10-24 (B21–B23):**
- `SquadDef.body_armour` and `aiming_aids`: dated rows, resolved when a squad spawns.
- `InfantryModel.night_aiming_factor` 1.6, scaled by darkness (1 − light).
- The coax is unchanged, because it is aimed through the vehicle's sights.

Two values differ from the table above:
- The Red 1PN58-class sight is dated from 1975. Its early-1970s introduction is in the table;
  the issue year is an estimate.
- AN/PAQ-4-class IR aiming lights from the late 1980s are not listed, because the sources here
  don't give their issue dates. Check before adding them: if they were issued with the PVS-7,
  Blue in 1990 would also get an aiming aid at night.

## 4. Open questions for alignment

Decided 2026-10-24: Q2 yes, body armour and night aiming go in with the era data as one step; Q3 yes, D2 is its own step before M8f1; Q4 no, enter `low`/`estimate` values with notes and upgrade them if the PDFs are obtained; Q1 keep the current lethality, run the harness (B25) before the M8b-b anti-tank teams are tuned and decide from its results.

1. Keep the lethality where it is (lenient side), or tighten it towards Rowland × Dupuy with a
   narrower aiming error or a smaller exposed area?
2. Body armour and night aiming need per-squad model fields. Do them with the era data, or defer?
3. Treat the sector search factor (D2) as a separate model-wide step with re-calibration?
4. Values in §3 are mostly `estimate`/`low`. Should we pull the primary PDFs (ARL-TR-8594,
   Holcomb 2006, Owens 2008) before entering them? DTIC blocked automated download during this
   research.
