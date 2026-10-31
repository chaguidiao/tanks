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
- **FM 6-0 *Commander and Staff Organization and Operations*** (2022) — orders, "on order" execution by code word, the execution matrix, branches and sequels (the checkpoint plans of DESIGN.md §8.3).
- **ADP 3-90 *Offense and Defense*** (2019) — control measures: checkpoints, routes, phase lines, battle positions.
- **TC 3-25.26 *Map Reading and Land Navigation*** — grid, contours, military crest, intervisibility.

- **FM 90-10-1 *An Infantryman's Guide to Combat in Built-up Areas*** (1993) — movement inside buildings, ch. 7 weapons effects and wall protection (building classes, `data/terrain/buildings.ron`).
- **ATP 3-21.8 *Infantry Platoon and Squad*** (2016) — squad formations, battle drill "React to contact" (squad AI, `data/combat/infantry.ron`).
- **FM 7-7J *Mechanized Infantry Platoon and Squad (Bradley)*** (1986) and **ATP 3-21.71 *Mechanized Infantry Platoon and Squad (Bradley)*** (2010) — dismounts per M2, dismount and security drills.
- **FM 3-22.68 *Crew-Served Machine Guns*** (2003) — rates of fire, burst lengths, ranges (squad MGs and vehicle coax).
- **FM 3-22.9 *Rifle Marksmanship*** (2008) — rifle rates of fire and effective ranges.
- **R. P. Hunnicutt, *Bradley: A History of American Fighting and Support Vehicles*** (1999) — M2/M3 stowage and troop compartment.
- **FM 3-21.18 *Foot Marches*** (2017) — rates of march by day and night, road and cross-country (`data/terrain/foot.ron`).

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
- **R. P. Hunnicutt, *Abrams: A History of the American Main Battle Tank*** (1990); **S. Zaloga** Osprey titles (T-72, T-80, M1, Leopard 2) — specs and development history. M2 Bradley, T-72A and M1 (1980) figures: open editions of Jane's Armour and Artillery, Zaloga's Osprey titles and Hunnicutt; armour values are open-source estimates marked `estimate` in the data. BMP-2 and FV107 Scimitar data: Zaloga, *BMP-1&BMP-2 Infantry Fighting Vehicle* (Osprey 1995) and open editions of Jane's Armour and Artillery; ERA and HEAT figures are open-source estimates marked `estimate`/`low` in the data.
- T-64B/BV, T-80B/BV, BRDM-2, M3 Bradley CFV, BMP-3/BRM-3K, 9M112 Kobra and 14.5 mm KPV figures (TODO I1, 2026-10): open-source summaries, mainly the Wikipedia articles *T-64*, *T-80*, *BRDM-2*, *M3 Bradley*, *BMP-3*, *9M112 Kobra* and *KPV heavy machine gun* (read October 2026), plus the armedconflicts.com BRM-3K entry for its mass, engine and sensors. These are secondary summaries, so the data marks them `low` at best and most armour, fire-control and mobility details `estimate`; check them against Zaloga's *T-80 Standard Tank* (Osprey New Vanguard 152, 2009) and *T-64 Battle Tank* (New Vanguard 223, 2015) and FM 100-2-3 before tuning.
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

## Infantry combat and detection (M8b; notes in docs/plans/M8B-CALIBRATION-NOTES.md)
- ★ **N. Hitchman, *Operational Requirements for an Infantry Hand Weapon*** (ORO-T-160, 1952) — rifle aiming error under mild stress (about 1.5–2 mrad, independent of range), hit probability on silhouettes, 0.3 kill per bullet hit, engagement and visibility ranges ([DTIC AD0000346 full text](https://archive.org/stream/DTIC_AD0000346/DTIC_AD0000346_djvu.txt)).
- ★ **G. W. Beebe & M. E. DeBakey, *Battle Casualties*** (1952) — casualties by agent (Tables 50–55), bullet lethality about 25 % of men hit (p. 155), position of men when hit (Table 72) ([PDF](https://www.generalstaff.org/BBOW/LOV-DUP/Battle_Casualties_Beebe-DeBakey.pdf)).
- ★ **D. Rowland**, "The Use of Historical Data in the Assessment of Combat Degradation" ([JORS 1987](https://link.springer.com/article/10.1057/jors.1987.27)), "The Effect of Combat Degradation on the Urban Battle" ([JORS 1991](https://link.springer.com/article/10.1057/jors.1991.113)), *The Stress of Battle* (TSO, 2006) — combat effectiveness 1/7 to 1/10 of field trials; summaries by the [Dupuy Institute](https://dupuyinstitute.org/tag/david-rowland/) and, secondary, a [wargaming site](https://testofbattle.com/upload/bob/Firepower.htm) (rifle against MG casualties).
- Rounds per enemy killed (secondary, not usable for aimed fire): [Interesting Engineering](https://interestingengineering.com/how-accurate-is-black-mirrors-men-against-fire), [HistoryNet on Lanning](https://historynet.com/book-review-nside-the-crosshairs-snipers-in-vietnam-by-michael-lee-lanning-vn/).
- Detection of soldiers: [US Army camouflage Phase IV test plan](https://soldiersystems.net/2010/06/30/army-camo-phase-iv/) (50 % detection distance, 35–500 m), [STANAG 4347 human target via a Johnson calculator](https://cctvplanner.io/johnson-criteria-calculator), [commercial thermal camera, human against vehicle range](https://www.asmag.com/product_detail.aspx?id=21478), NVESD [search and detection modelling](https://experts.arizona.edu/en/publications/search-and-detection-modeling-of-military-imaging-systems/) and [time-limited search](https://experts.arizona.edu/en/publications/nvesd-time-limited-search-model/), Flug, [detection of humans in rural terrain](https://neurophotonics.spiedigitallibrary.org/conference-proceedings-of-spie/9071/90710M/Modeling-static-and-dynamic-detection-of-humans-in-rural-terrain/10.1117/12.2053720.full) (SPIE 9071, 2014).
- Era changes to 2005: ARL-TR-8594, *Effects of Sight Type, Zero Methodology, and Target Distance on Shooting Performance* (2018, [DTIC AD1064518](https://apps.dtic.mil/sti/pdfs/AD1064518.pdf)); AN/PAS-13 ranges and fielding ([Strategypage 2016](https://www.strategypage.com/htmw/htecm/20160424.aspx), [Defense Industry Daily](https://www.defenseindustrydaily.com/?p=3420)); [1PN58 night sight](https://en.wikipedia.org/wiki/1PN58); OIF/OEF wound distribution ([UTMB record of Owens et al. 2008](https://researchexperts.utmb.edu/en/publications/combat-wounds-in-operation-iraqi-freedom-and-operation-enduring-f/)); Holcomb et al., "Understanding combat casualty care statistics", *J Trauma* 60 (2006) 397–401 (case fatality rates by war); Ehrhart, *Increasing Small Arms Lethality in Afghanistan* (SAMS, 2009, [DTIC ADA512331](https://apps.dtic.mil/sti/pdfs/ADA512331.pdf)).

## Anti-tank weapons and vehicle vulnerability (M8b-b; notes in docs/plans/M8B-AT-TEAMS.md §12)
- ★ **L. Grau, "Russian-Manufactured Armored Vehicle Vulnerability in Urban Combat: The Chechnya Experience"** (FMSO, *Red Thrust Star*, 1997). Covers losses, kills on areas without ERA, 3–6 lethal hits per destroyed vehicle, and hunter-killer teams. Copies: [FAS](https://man.fas.org/dod-101/sys/land/row/rusav.htm), [GlobalSecurity](https://www.globalsecurity.org/military/library/report/1997/rusav.htm). Also Grau, *Russian Urban Tactics: Lessons from the Battle for Grozny* ([DTIC ADA394517](https://apps.dtic.mil/sti/pdfs/ADA394517.pdf)).
- **D. Isby, *Weapons and Tactics of the Soviet Army*** (Jane's, 1981/1988). RPG-7 first-round hit table by range. Seen only via a [forum transcription](https://community.battlefront.com/topic/91032-why-use-an-m136at4-when-you-have-a-javelin/page/2) and [generalstaff.org](https://generalstaff.org/BBOW/Weapons/Hist_PHitsPKills.htm) (secondary; check against the book). Also a 1975–76 US Army moving-panel evaluation and gunners' 10–15 % range-estimation error.
- **FM 3-23.25 *Light Antiarmor Weapons*** (2001), [archive.org](https://archive.org/download/MManuals/UsArmyLightAntiarmorWeapons2001.pdf). M72 and AT4 ranges and penetration.
- Iraq 2003 Abrams damage (PM Abrams report via Jane's; secondary: [ARRSE](https://www.arrse.co.uk/community/threads/americans-dont-know-what-hit-them.3525/post-53095), [StrategyPage](https://www.strategypage.com/htmw/htarm/articles/20030410.aspx)).
- **FM 23-24 *M47 Dragon Medium Antitank Weapon System*** (1987, archive), **FM 3-22.37 *Javelin*** (2003/2008), **FM 100-2-3 *The Soviet Army: Troops, Organization and Equipment*** (1991): team organisation, ranges, set-up.
- **L. Grau and T. Thomas, "Russian Lessons Learned from the Battles for Grozny"** (FMSO, 2000): RPGs fired from rooms and basements.
- Launcher and round data, `low`, from Wikipedia: [RPG-7](https://en.wikipedia.org/wiki/RPG-7) (rocket table; the 1976 US Army 5 x 2.5 m moving-panel hit table), [RPG-18](https://en.wikipedia.org/wiki/RPG-18), [RPG-26](https://en.wikipedia.org/wiki/RPG-26), [M72 LAW](https://en.wikipedia.org/wiki/M72_LAW) (FM 3-23.25 and FM 7-7 figures; effective range = hit chance one half), [M47 Dragon](https://en.wikipedia.org/wiki/M47_Dragon) (Dragon II 1988, 610 mm), [9K111 Fagot](https://en.wikipedia.org/wiki/9K111_Fagot), [9M133 Kornet](https://en.wikipedia.org/wiki/9M133_Kornet), [FGM-148 Javelin](https://en.wikipedia.org/wiki/FGM-148_Javelin). Fetched 2026-10-28.
- ATGM performance: Sagger 1973 (Iyer 1995, via [Wikipedia, MCLOS](https://en.wikipedia.org/wiki/Manual_command_to_line_of_sight)); Javelin in Ukraine, 100 hits from 112 shots ([National Interest](https://nationalinterest.org/node/213728)); system data from [CSIS Missile Threat](https://missilethreat.csis.org/missile/fgm-148-javelin/) and the Wikipedia articles on the Dragon, Fagot, Kornet, Kontakt-5 and AT4, plus [GlobalSecurity AT-7](https://www.globalsecurity.org/military/world/russia/at-7.htm) (`low`).

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
- **ATP 3-09.50** *The Field Artillery Cannon Battery* (2016) — battery ammunition resupply from the ammunition transfer point (ch. 4) and survivability moves after firing (ch. 2): the basis of the C3 resupply and displacement timings in `data/combat/fires.ron` (values `estimate`).
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
- eSim *Steel Beasts Pro PE* — routes whose waypoints set tactics, formation, speed and fire control, with conditions to leave a battle position ([SBWiki: Moving units on routes](https://www.steelbeasts.com/sbwiki/index.php/Moving_Units_On_Routes)); Holdridge *TacOps* — orders attached to waypoints, with an estimated time at each ([TacOps 4 user guide](https://generaltzsoschickencoop.wordpress.com/wp-content/uploads/2009/08/tacops4-guide-user.pdf)); Battlefront *Combat Mission* series; Graviteam *Tactics* series; *Flashpoint Campaigns*

## Historical validation cases
- 73 Easting (1991) — the IDA/DARPA SIMNET reconstruction.
- Golan Heights, 1973 (Valley of Tears) — Kahalani, *The Heights of Courage*.
- Medina Ridge (1991); Norfolk (1991).
