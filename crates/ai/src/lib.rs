//! Vehicle, platoon and commander AI. Pure functions over plain inputs: the AI reads only a
//! side's `PerceivedWorld` and its own vehicles' state, never ground truth (CLAUDE.md rule 3).

pub mod commander;
pub mod drive;
pub mod firedist;
pub mod formation;
pub mod picture;
pub mod position;
