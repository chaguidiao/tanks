# References

A reading list grouped by subsystem. ★ = read first. Public sources only — never use restricted or classified material.

## Doctrine — Blue (friendly)
- ★ **ATP 3-20.15 *Tank Platoon*** (2025, supersedes 2019). Formations, movement techniques, battle drills (App. F), defence/BP occupation, fire distribution. armypubs.army.mil
- **FM 17-15 *Tank Platoon*** (1987 & 1996 editions) — Cold War predecessor; ideal if the era is ~1990.
- **ATP 3-90.1 *Armor and Mechanized Infantry Company Team*** — the company echelon the player commands.
- **FM 71-1 *Tank and Mechanized Infantry Company Team*** (1998) — Cold War/90s company TTPs.
- **FM 3-90 *Tactics*** (2023) — offence/defence tasks, control measures.
- **TC 3-20.31 *Training and Qualification, Crew*** — tank gunnery: fire commands, engagement standards and times.
- **ATP 2-01.3 *Intelligence Preparation of the Battlefield*** — OAKOC terrain analysis (what the AI's terrain reasoning should mimic).
- **FM 1-02.2 *Military Symbols*** and **MIL-STD-2525D/E**, **NATO APP-6D/E** — symbology and tactical graphics.
- **TC 3-25.26 *Map Reading and Land Navigation*** — grid, contours, military crest, intervisibility.

## Doctrine — Red (OPFOR)
- ★ **FM 100-2-1 *The Soviet Army: Operations and Tactics*** (1984) — march → pre-battle → battle formations, norms.
- **FM 100-2-3 *The Soviet Army: Troops, Organization and Equipment*** (1991).
- **TC 7-100.2 *Opposing Force Tactics*** (2011); **ATP 7-100.1 *Russian Tactics*** (2024).
- **Grau & Bartles, *The Russian Way of War*** (FMSO, 2016) — free PDF.

## Combat modelling (how professional sims do it)
- ★ **Washburn & Kress, *Combat Modeling*** (Springer, 2009) — detection, search theory, hit/kill probability, Lanchester.
- **Tolk (ed.), *Engineering Principles of Combat Modeling and Distributed Simulation*** (Wiley, 2012).
- **OneSAF / ModSAF / JANUS / JCATS** documentation and papers (DTIC) — computer-generated-forces architecture: task frames, behaviour hierarchy, terrain reasoning.
- **Driels, *Weaponeering: Conventional Weapon System Effectiveness*** (AIAA, 2nd ed. 2013) — error budgets, delivery accuracy, Pk.

## Vehicles & mobility
- ★ **R. M. Ogorkiewicz, *Technology of Tanks*** (Jane's, 1991) and ***Tanks: 100 Years of Evolution*** (Osprey, 2015) — mobility, armour, fire control, hit probability.
- **J. Y. Wong, *Theory of Ground Vehicles*** (Wiley, 5th ed.) — tracked vehicle traction, steering, resistance.
- **M. G. Bekker, *Theory of Land Locomotion* / *Off-the-Road Locomotion*** — terramechanics.
- **NATO AVT-248, *Next-Generation NATO Reference Mobility Model (NG-NRMM)*** final report (2018) — speed-made-good over terrain, soil cone index, slope, vegetation override.
- **R. P. Hunnicutt, *Abrams: A History of the American Main Battle Tank*** (1990); **S. Zaloga** Osprey titles (T-72, T-80, M1, Leopard 2) — specs and development history. BMP-2 and FV107 Scimitar data: Zaloga, *BMP-1&BMP-2 Infantry Fighting Vehicle* (Osprey 1995) and open editions of Jane's Armour and Artillery; ERA and HEAT figures are open-source estimates marked `estimate`/`low` in the data.
- Manufacturer brochures, defence-expo data sheets, Jane's Armour & Artillery (older editions) — cite edition/year per stat.

## Ballistics & terminal effects
- **R. L. McCoy, *Modern Exterior Ballistics*** (1999) — point-mass trajectory, drag.
- **Carlucci & Jacobson, *Ballistics: Theory and Design of Guns and Ammunition*** (CRC, 3rd ed. 2018) — interior/exterior/terminal overview.
- ★ **W. Lanz & W. Odermatt**, "Penetration limits of conventional large caliber anti tank guns / kinetic energy projectiles" (Int. Symposium on Ballistics, 1992) and later Odermatt papers — long-rod penetration formula.
- **Walters & Zukas, *Fundamentals of Shaped Charges*** (1989); **M. Held** papers on ERA and shaped charges.
- US Army *Engineering Design Handbook* (AMCP 706 series, DTIC) — fire control and ballistics volumes.

## Sensors, detection & target acquisition
- ★ **J. Johnson**, "Analysis of Image Forming Systems" (1958) — Johnson criteria.
- **Vollmerhausen & Jacobs**, "The Targeting Task Performance (TTP) Metric" (NVESD, 2004).
- **Driggers, Friedman & Nichols, *Introduction to Infrared and Electro-Optical Systems*** (Artech, 2nd ed. 2012).
- NVESD **ACQUIRE** search model documentation (time-to-detect).

## Line of sight & terrain algorithms
- **Franklin & Ray**, "Higher isn't necessarily better: visibility algorithms and experiments" (1994) — R2, XDraw.
- **Van Kreveld**, "Variations on sweep algorithms: efficient computation of extended viewsheds and class intervals" (1996).
- **Wang, Robinson & White**, "Generating viewsheds without using sightlines" (PE&RS, 2000).
- Marching squares / contouring — any GIS text; `contour` crate docs.

## Game AI
- ★ ***Game AI Pro* 1–3 + 360** (free at gameaipro.com): HTN planners (Humphreys), utility AI (Mark/Dill), tactical position selection, influence maps (Mark), flow fields (Emerson), behaviour trees.
- ★ **Straatman, van der Sterren & Beij**, "Killzone's AI: Dynamic Procedural Combat Tactics" (GDC 2005) — tactical position evaluation; the model for our hull-down finder.
- **Nau et al.**, "SHOP2: An HTN Planning System" (JAIR, 2003).
- **D. Mark, *Behavioral Mathematics for Game AI*** (2009); **Mark & Dill**, "Improving AI Decision Modeling Through Utility Theory" (GDC 2010).
- **D. Pottinger**, "Coordinated Unit Movement" / "Implementing Coordinated Movement" (Gamasutra, 1999).
- **R. C. Coulter**, "Implementation of the Pure Pursuit Path Tracking Algorithm" (CMU-RI-TR-92-01, 1992) — route following in `ai::drive`.
- **Botea, Müller & Schaeffer**, "Near Optimal Hierarchical Path-Finding" (HPA*, 2004).
- **J. Orkin**, "Three States and a Plan: The AI of F.E.A.R." (GDC 2006) — GOAP, for contrast.
- **D. Isla**, "Handling Complexity in the Halo 2 AI" (GDC 2005) — behaviour trees.
- **Ontañón et al.**, "A Survey of Real-Time Strategy Game AI Research and Competition in StarCraft" (IEEE TCIAIG, 2013); **Vinyals et al.**, AlphaStar (Nature, 2019) — why learned policies are not our v1 approach.

## Smoke, weather, light and fire support (M7)
- **FM 3-11.50 / ATP 3-11.50**, *Obscuration Operations* — smoke effects, screening, cloud behaviour in wind; tank self-screening doctrine in **ATP 3-20.15** (React to contact: employ smoke).
- **Cooper (1969)** solar declination approximation, `δ = 23.44°·sin(360°/365·(N−81))`; sun elevation from hour angle (any solar-position text, e.g. Duffie & Beckman). Moon and twilight light levels (civil −6°, nautical −12°, astronomical −18°) are standard definitions; the optical light curve is an `estimate`.
- **Koschmieder** relation: extinction k = 3.912 / V (meteorological visibility V, 2 % contrast) — fog/haze/rain presets in `data/combat/weather.ron`.
- **Beer–Lambert** law for smoke transmittance T = exp(-k * CL), concentration-length; smoke mass extinction coefficients in open literature on HC and red-phosphorus screens (e.g. Palmer & Williams, *Appl. Opt.* 1974 on IR extinction of fog and smoke).
- **J. Meeus**, *Astronomical Algorithms* (2nd ed., 1998) — low-precision solar position for sun elevation from date, time, latitude.
- **Johnson / NVESD** and thermal-sight literature — thermal crossover at dawn and dusk (target-background contrast passes through zero).
- **ATP 3-09.30** *Observers*, **ATP 3-09.42** *Fire Support for the Brigade Combat Team*, **FM 6-40** — call-for-fire format (observer ID, warning order, target location, description, method of engagement and control), SHOT / SPLASH / ROUNDS COMPLETE.
- **ATP 3-20.15** *Tank Platoon* and **FM 3-90.1** — break contact (suppress, smoke, bound back), react to indirect fire.
- Ranging errors: firing-table probable errors (about 0.3–0.5 % of range, σ = 1.48 × PE) from **FM 6-40** / **TC 3-09.81**; battery ranges and rates from open descriptions of the M109A6 and 2S3. Effects of HE on armour (`data/combat/fires.ron`) are `estimate`.
- Suppression: Dunnigan, Marshall and later suppression-model discussions are *qualitative only*; values in data are `estimate`.

## Symbology & presentation
- **milsymbol** (MIT): https://github.com/spatialillusions/milsymbol — 2525/APP-6 SVG generation.
- **raylib** (zlib): https://www.raylib.com — `raylib-rs` bindings: https://docs.rs/raylib
- Fonts (OFL): IBM Plex Mono, JetBrains Mono, Share Tech Mono.

## Terrain data
- Procedural generation and hydrology papers: see `docs/TERRAIN.md` §10.
- USGS **3DEP** (1 m lidar / 10 m), **Copernicus DEM GLO-30**, **EU-DEM**, national open lidar (DE, FI, NL, …), **OpenTopography** portal.
- **OpenStreetMap** (roads, forest, water, buildings); **ESA WorldCover** 10 m land cover.

## Games to study
- ProSIM *ATF* / *BCT Commander* — https://www.shrapnelgames.com/ProSIM/ATF/ATF_page.html
- MicroProse *M1 Tank Platoon* (1989) and *M1 Tank Platoon II* (1998)
- Matrix *Armored Brigade* (2018) and *Armored Brigade II* (2024/25) — pausable real-time, real-world terrain, 1965–1991
- eSim *Steel Beasts Pro PE*; Battlefront *Combat Mission* series; Graviteam *Tactics* series; *Flashpoint Campaigns*

## Historical validation cases
- 73 Easting (1991) — the IDA/DARPA SIMNET reconstruction.
- Golan Heights, 1973 (Valley of Tears) — Kahalani, *The Heights of Courage*.
- Medina Ridge (1991); Norfolk (1991).
