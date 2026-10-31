# Tanks

> *Working title. The name was chosen in about four seconds, and it shows. It is the video game equivalent of a cardboard box labelled "THINGS".*

A top-down, real-time-with-pause tank command simulation. You give platoons intent-level orders, the vehicle and platoon AI tries to carry them out using doctrine-based tactics, and the terrain is generated from real-world geodata so that the hill you wanted to hide behind is actually there. Mostly. Usually. Depending on the seed.

Serious tone, tactical-display aesthetics, physically grounded models. The tone is serious. The author is not.

## Read this first

This is a **weekend project** that got out of hand, made by a **non-specialist**.

- **Not a subject matter expert.** The author has no military, ballistics, armour or doctrine background. The doctrine packs, penetration maths and vehicle numbers are a hobbyist's best reading of public sources. Some of it is wrong. Some of it is confidently wrong. Each value says how sure it is (`high`, `medium`, `low` or `estimate`), and `estimate` means "somebody guessed, and wrote down that they guessed".
- **Built with Claude.** Hello. I'm Claude, an AI model made by Anthropic, and I wrote most of the code, docs and this paragraph under the direction of a human who asked good questions and occasionally said "no, that's wrong". Expect AI-generated code, AI-generated doctrine summaries, and AI-generated confidence. Please check them before trusting them, the same way you would check anyone else who sounds sure of themselves.
- **No commercial intent.** There is no company, product, roadmap, sales funnel, support desk, Discord, or plan to sell anything. It is MIT licensed so that you can do what you like with it, including ignoring it.
- **Not for real-world use.** Nothing here is intended, validated or suitable for training, planning, targeting, operations, or any other real-world military or safety purpose. It is a game-like simulation. Treat its numbers as a toy, and check the cited sources before relying on any of them for anything that matters.
- **No support promises.** Issues are read when the weekend allows. Pull requests are welcome, but please keep expectations at "hobby project" level.

If you came here expecting a finished, validated, professionally engineered wargame, this is the wrong repository. If you came here to poke at a strange little tactical sim that someone built on weekends with an AI for company, welcome aboard.

## Status

Early development, version 0.1.2. In so far: the core simulation (detection, gunnery, armour and damage), generated and editable terrain including a market town, doctrine-driven platoon and commander AI, checkpoint plans and code words, smoke, weather and night, called artillery, infantry squads with their carriers, anti-tank teams and ambushes, and the first pieces of logistics (supply points, LOGPACs, battery resupply). Sixteen scenarios from 1985 to 2005, a scenario and terrain editor, an after-action review and a database of every number with its source. Open work lives in [`docs/TODO.md`](docs/TODO.md) and the design and decision log in [`docs/DESIGN.md`](docs/DESIGN.md). Those documents are more honest about what works than this README is.

Known things that are definitely not finished: logistics is half built (no supply trucks or vehicle recovery yet), nothing shoots back at artillery, engineers and obstacles do not exist, and the AI sometimes does things an actual platoon commander would find hilarious.

## Building

The project is a Rust workspace (`crates/sim`, `terrain`, `terragen`, `ai`, `sim-data`, `client`, `headless`). All builds and tests run inside a podman development container, so your host stays clean and the author's laptop stays unexploded. See [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md).

Prebuilt packages for Linux, Windows and macOS (Apple silicon) are on the [Releases](https://github.com/chaguidiao/tanks/releases) page, built by the release workflow (see the Releases section of `docs/DEVELOPMENT.md`). They are unsigned, so Windows SmartScreen and macOS Gatekeeper will complain once. The player's guide is a PDF built from `docs/guide/`.

## Documentation

- [`docs/DESIGN.md`](docs/DESIGN.md): simulation design, doctrine model, decisions (and the reasons for them, some of which were "it seemed fine at the time")
- [`docs/TERRAIN.md`](docs/TERRAIN.md): terrain authoring, baking, generation and editor
- [`docs/REFERENCES.md`](docs/REFERENCES.md): public sources for doctrine, vehicle data and combat modelling
- [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md): development environment and commands

## Data and sources

Vehicle, ammunition and doctrine data live in `data/**/*.ron`. Every numeric value carries its source and a confidence level. Only public sources are used. Vehicles are fictional designs that mirror real equipment; the real designs are named only so that you can find out what they are. The fictional names are not affiliated with any real manufacturer or military. See [`NOTICE`](NOTICE) for the full attribution and disclaimers.

If a number has no public source, it says `estimate` and explains why. If you find a number that says `high` and is wrong, please open an issue. Nobody is more embarrassed about this than the AI, and it will try very hard to apologise.

## Contributing

Issues and pull requests are welcome, with the following expectations:

- Commit messages explain the *why*, because future-you will otherwise forget and future-Claude will guess.
- Tests, `cargo fmt`, `cargo clippy` and `cargo nextest` pass before you send anything.
- No real-world operational data. Public sources only, cited properly.
- Please don't ask when the next release is. The answer is "after the weekend".

## Licence

Code and original data are released under the MIT licence; see [`LICENSE`](LICENSE). Third-party components and their licences are listed in [`THIRD_PARTY.md`](THIRD_PARTY.md). Trademark, publication and attribution notices are in [`NOTICE`](NOTICE).

---

*Made on weekends, by a human and Claude, with no commercial intent and no expertise that would survive a serious review. The tanks are fictional. The disclaimers are not.*
