//! Vehicle and platoon AI. Pure functions over plain inputs: the AI never sees `TruthWorld`;
//! `sim` feeds it the vehicle's own state and what its side perceives (CLAUDE.md rule 3).

pub mod drive;
