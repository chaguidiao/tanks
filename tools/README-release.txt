TANKS (working title) - top-down tank command simulation
=======================================================

Run
  tanks            the game (OpenGL 3.3)
  tanks-gl21       the same game built for OpenGL 2.1 (older graphics hardware, about 2013)
  tanks-headless   command-line tool: run scenarios without a window, balance batches, map tools

Start tanks from this folder or from anywhere: it finds the data/ folder next to the program
(or set TANKS_DATA to another data folder). The title screen leads to SCENARIOS; press F7 in a
scenario for help. The maps are included in data/maps; they are generated from recipes and
seeds and are rebuilt automatically if missing.

User guide
  USER-GUIDE.pdf if it is in this folder, otherwise download tanks-<version>-user-guide.pdf from
  the release page: https://github.com/chaguidiao/tanks/releases

Scenarios (data/scenarios)
  cw-village-defence, cw-night-delay, cw-hasty-attack     Cold War (1986-87), M1 / T-72 era
  mod-fog-meeting, mod-dusk-attack, mod-delay-in-depth    Modern (2005), M1A2 / T-90 era
  m8f-ambush, m8h-town                                    infantry with the tanks: a village, a town
  m1-move ... m7d-fire-support, m8b-*                     short tutorials of single systems

Linux needs OpenGL drivers and the usual X11 or Wayland libraries. macOS: if Gatekeeper blocks
the unsigned program, allow it in System Settings, Privacy and Security.

All vehicle and ammunition data are public-source estimates with sources and confidence in the
data files; vehicle names are fictional. Not for any real-world use. Licence: MIT (LICENSE);
disclaimers and attribution in NOTICE; third-party assets in THIRD_PARTY.md.
