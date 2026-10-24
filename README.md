# Tanks

A top-down, real-time-with-pause tank command simulation. The player commands tank platoons with intent-level orders, and vehicle and platoon AI carries them out using doctrine-based tactics, on terrain modelled from geodata.

Working title. Serious tone, tactical-display aesthetics, physically grounded models.

## Status

Early development. Core simulation, terrain generation and editing, AI tactics, and the first scenarios are implemented; see [`docs/TODO.md`](docs/TODO.md) for open work and [`docs/DESIGN.md`](docs/DESIGN.md) for the design and decision log.

## Building

The project is a Rust workspace (`crates/sim`, `terrain`, `terragen`, `ai`, `sim-data`, `client`, `headless`). All builds and tests run inside a podman development container so the host stays clean. See [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md).

Prebuilt packages for Linux, Windows and macOS are produced by the release workflow (see the Releases section of `docs/DEVELOPMENT.md`).

## Documentation

- [`docs/DESIGN.md`](docs/DESIGN.md): simulation design, doctrine model, decisions
- [`docs/TERRAIN.md`](docs/TERRAIN.md): terrain authoring, baking, generation and editor
- [`docs/REFERENCES.md`](docs/REFERENCES.md): public sources for doctrine, vehicle data and combat modelling
- [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md): development environment and commands

## Data and sources

Vehicle, ammunition and doctrine data live in `data/**/*.ron`. Every numeric value carries its source and a confidence level (`high`, `medium`, `low` or `estimate`). Only public sources are used. Vehicles are fictional designs that mirror real equipment, and the real designs are named only to identify them.

## Licence

Code and original data are released under the MIT licence; see [`LICENSE`](LICENSE). Third-party components and their licences are listed in [`THIRD_PARTY.md`](THIRD_PARTY.md). Trademark and attribution notices are in [`NOTICE`](NOTICE).
