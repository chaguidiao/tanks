//! Editing control measures and contingency orders after placement (TODO J5): move, resize and
//! turn a drawn graphic (it keeps its id, so triggers and plans that refer to it follow), and
//! re-time, re-attach or cancel a contingency that has not fired. Everything goes through
//! `Command::UpdateGraphic` / `UpdateTrigger` / `RemoveTrigger`: control measures, no radio delay.

use super::ctx::MenuAct;
use super::*;
use crate::hud::MenuItem;

/// Radius step of the graphic menu (m).
const RADIUS_STEP_M: f64 = 100.0;
const RADIUS_M: (f64, f64) = (50.0, 2000.0);
/// Re-timing step of an AT-time contingency (s).
const RETIME_S: f64 = 300.0;

/// An edit to a graphic or a contingency (context menu items).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GfxEdit {
    /// Pick the graphic up: the next left click puts it down there.
    Move(u32),
    /// Grow (+1) or shrink (-1) an objective or battle position.
    Radius(u32, i8),
    /// Turn a battle position's threat direction by 45° clockwise.
    Turn(u32),
    CancelTrigger(u32),
    /// Move an AT-time contingency later (+1) or earlier (-1) by `RETIME_S`.
    Retime(u32, i8),
    /// Attach a phase-line contingency to another phase line.
    OnLine(u32, u32),
}

/// The point a graphic is picked up by: its centre, the TRP, or the middle of a phase line.
fn anchor(k: &SimGraphic) -> [f64; 2] {
    match k {
        SimGraphic::Trp { at } => *at,
        SimGraphic::Objective { centre, .. } | SimGraphic::BattlePosition { centre, .. } => *centre,
        SimGraphic::PhaseLine { points } => {
            let n = points.len().max(1) as f64;
            [points.iter().map(|p| p[0]).sum::<f64>() / n, points.iter().map(|p| p[1]).sum::<f64>() / n]
        }
    }
}

fn translated(k: &SimGraphic, d: [f64; 2]) -> SimGraphic {
    let sh = |p: [f64; 2]| [p[0] + d[0], p[1] + d[1]];
    match k {
        SimGraphic::Trp { at } => SimGraphic::Trp { at: sh(*at) },
        SimGraphic::Objective { centre, radius } => SimGraphic::Objective { centre: sh(*centre), radius: *radius },
        SimGraphic::BattlePosition { centre, radius, facing } => {
            SimGraphic::BattlePosition { centre: sh(*centre), radius: *radius, facing: *facing }
        }
        SimGraphic::PhaseLine { points } => SimGraphic::PhaseLine { points: points.iter().map(|p| sh(*p)).collect() },
    }
}

impl Game {
    /// Edit items for the graphic menu.
    pub(super) fn graphic_edit_items(&self, id: u32) -> Vec<MenuItem<MenuAct>> {
        let Some(g) = self.picture().graphic(id) else { return Vec::new() };
        let e = |l: &str, a: GfxEdit| MenuItem::new(l, MenuAct::Gfx(a));
        let mut v = vec![e("MOVE GRAPHIC", GfxEdit::Move(id))];
        if matches!(g.kind, SimGraphic::Objective { .. } | SimGraphic::BattlePosition { .. }) {
            v.push(e("RADIUS +100 M", GfxEdit::Radius(id, 1)));
            v.push(e("RADIUS -100 M", GfxEdit::Radius(id, -1)));
        }
        if matches!(g.kind, SimGraphic::BattlePosition { .. }) {
            v.push(e("TURN 45°", GfxEdit::Turn(id)));
        }
        v
    }

    /// Menu of a contingency order (from its row in the selection panel).
    pub(super) fn trigger_menu(&self, id: u32) -> (String, Vec<MenuItem<MenuAct>>) {
        let pic = self.picture();
        let Some(t) = pic.triggers.iter().find(|t| t.id == id) else {
            return ("CONTINGENCY".into(), Vec::new());
        };
        let title = format!("CONT {}: {} -> {}", t.id, self.condition_text(&t.condition), t.order);
        let e = |l: String, a: GfxEdit| MenuItem::new(l, MenuAct::Gfx(a));
        let mut v = Vec::new();
        match &t.condition {
            TriggerCondition::AtTime { .. } => {
                v.push(e("5 MIN LATER".into(), GfxEdit::Retime(id, 1)));
                v.push(e("5 MIN EARLIER".into(), GfxEdit::Retime(id, -1)));
            }
            TriggerCondition::EnemyCrosses { line } => {
                let lines: Vec<MenuItem<MenuAct>> = pic
                    .graphics
                    .iter()
                    .filter(|g| matches!(g.kind, SimGraphic::PhaseLine { .. }))
                    .map(|g| {
                        e(format!("ON ENEMY CROSSING {}", g.label), GfxEdit::OnLine(id, g.id)).checked(g.id == *line)
                    })
                    .collect();
                v.push(MenuItem::submenu("PHASE LINE", lines));
            }
            _ => {}
        }
        v.push(e("CANCEL".into(), GfxEdit::CancelTrigger(id)).key("X"));
        (title, v)
    }

    pub(super) fn run_gfx_edit(&mut self, a: GfxEdit) {
        let pic = self.picture();
        match a {
            GfxEdit::Move(id) => {
                if pic.graphic(id).is_some() {
                    self.tool = Tool::MoveGraphic { id };
                }
            }
            GfxEdit::Radius(id, dir) => {
                let Some(g) = pic.graphic(id).cloned() else { return };
                let step = |r: f64| (r + f64::from(dir) * RADIUS_STEP_M).clamp(RADIUS_M.0, RADIUS_M.1);
                let kind = match g.kind {
                    SimGraphic::Objective { centre, radius } => SimGraphic::Objective { centre, radius: step(radius) },
                    SimGraphic::BattlePosition { centre, radius, facing } => {
                        SimGraphic::BattlePosition { centre, radius: step(radius), facing }
                    }
                    _ => return,
                };
                self.update_graphic(id, g.label, kind);
            }
            GfxEdit::Turn(id) => {
                let Some(g) = pic.graphic(id).cloned() else { return };
                let SimGraphic::BattlePosition { centre, radius, facing } = g.kind else { return };
                let facing = (facing + std::f64::consts::FRAC_PI_4).rem_euclid(std::f64::consts::TAU);
                self.update_graphic(id, g.label, SimGraphic::BattlePosition { centre, radius, facing });
            }
            GfxEdit::CancelTrigger(id) => {
                self.sim.submit_immediate(self.side, Command::RemoveTrigger { id });
                self.sim.settle();
                self.radio(format!("CONTINGENCY {id} CANCELLED."));
            }
            GfxEdit::Retime(id, dir) => {
                let Some(TriggerCondition::AtTime { time_s }) =
                    pic.triggers.iter().find(|t| t.id == id).map(|t| t.condition.clone())
                else {
                    return;
                };
                let t = (time_s + f64::from(dir) * RETIME_S).max(self.sim.time_s());
                self.update_trigger(id, TriggerCondition::AtTime { time_s: t });
                self.radio(format!("CONTINGENCY {id}: AT {}.", format::clock(self.scenario.start_time_s as f64 + t)));
            }
            GfxEdit::OnLine(id, line) => {
                let label = pic.graphic(line).map_or("PL".into(), |g| g.label.clone());
                self.update_trigger(id, TriggerCondition::EnemyCrosses { line });
                self.radio(format!("CONTINGENCY {id}: ON ENEMY CROSSING {label}."));
            }
        }
    }

    /// Put a picked-up graphic down so its anchor is at `to`.
    pub(super) fn place_moved_graphic(&mut self, id: u32, to: [f64; 2]) {
        let Some(g) = self.picture().graphic(id).cloned() else { return };
        let a = anchor(&g.kind);
        let kind = translated(&g.kind, [to[0] - a[0], to[1] - a[1]]);
        self.update_graphic(id, g.label, kind);
    }

    fn update_graphic(&mut self, id: u32, label: String, kind: SimGraphic) {
        self.sim.submit_immediate(
            self.side,
            Command::UpdateGraphic { id, graphic: Graphic { id, label: label.clone(), kind } },
        );
        self.sim.settle();
        self.radio(format!("GRAPHIC {label} AMENDED."));
    }

    fn update_trigger(&mut self, id: u32, condition: TriggerCondition) {
        self.sim.submit_immediate(self.side, Command::UpdateTrigger { id, condition });
        self.sim.settle();
    }

    /// Selection panel rows: the latest contingencies; open ones are buttons that open their
    /// menu, fired ones are footer text.
    pub(super) fn trigger_rows(
        &self,
        x: f32,
        mut y: f32,
        w: f32,
        foot: &mut Vec<(String, Rgba)>,
    ) -> (Vec<Button<GameAction>>, f32) {
        let mut rows = Vec::new();
        for t in self.picture().triggers.iter().rev().take(3) {
            let label = format!("CONT {}: {} -> {}", t.id, self.condition_text(&t.condition), t.order);
            if t.fired {
                foot.push((format!("{label} (DONE)"), style::TEXT_DIM));
                continue;
            }
            rows.push(
                Button::new(Rect::new(x + 6.0, y, w - 12.0, 20.0), Look::Row, label, GameAction::Contingency(t.id))
                    .colour(style::TEXT_DIM)
                    .tip("CHANGE OR CANCEL THIS CONTINGENCY ORDER"),
            );
            y += 22.0;
        }
        (rows, y)
    }
}
