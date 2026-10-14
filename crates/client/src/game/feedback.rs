//! Feedback and feedforward (docs/plans/CONTROL-UX.md §5.6): the top-bar hold state, the hold
//! ring and the mouse hint line. The hint is built from the same state the click handling
//! reads, so it says what the next click will do.

use super::ctx::MenuTarget;
use super::*;

/// Ring radius (px) and arc segments of the right-button hold ring.
const RING_R: f32 = 16.0;
const RING_SEGS: usize = 28;

impl Game {
    /// Why `pause_on_menu` holds the clock, if it does (shown in the top bar).
    pub(super) fn hold_reason(&self) -> Option<&'static str> {
        if self.paused || self.planning || !self.pause_on_menu {
            return None;
        }
        if self.menu.is_some() {
            Some("MENU")
        } else if self.plan.is_some() {
            Some("EDIT")
        } else {
            None
        }
    }

    /// Hold ring around the cursor while the right button is held: fills over the hold time.
    pub(super) fn draw_hold_ring(&self, f: &mut Frame) {
        let Some((at, frac)) = self.rmb_hold() else { return };
        f.circle_lines(at, RING_R, style::TEXT_DIM);
        let n = ((frac * RING_SEGS as f32).ceil() as usize).min(RING_SEGS);
        let pt = |k: usize| {
            let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * (k as f32 / RING_SEGS as f32).min(frac);
            [at[0] + RING_R * a.cos(), at[1] + RING_R * a.sin()]
        };
        for k in 0..n {
            f.line(pt(k), pt(k + 1), 3.0, style::AMBER);
        }
    }

    /// Mouse hint line for the cursor at `inp.mouse`.
    pub(super) fn compute_hint(&self, inp: &Input, cam: &Camera, over_ui: bool) -> String {
        if over_ui || self.menu.is_some() || self.aar.is_some() {
            return String::new();
        }
        let n = self.draft.len();
        let tool = match &self.tool {
            Tool::Move => Some("LMB MOVE HERE · RMB / ESC CANCEL".to_owned()),
            Tool::Route => Some(format!("LMB ADD CP ({n}) · RMB DESTINATION, SENDS · BKSP UNDO · ESC CANCEL")),
            Tool::Engage => Some("LMB ENGAGE CONTACT · RMB / ESC CANCEL".to_owned()),
            Tool::SmokeRound => Some("LMB FIRE SMOKE ROUND HERE · RMB / ESC CANCEL".to_owned()),
            Tool::CallFire => Some(format!(
                "LMB CALL FIRE ({}) HERE · Y HE / SMOKE · RMB / ESC CANCEL",
                if self.fire_kind == FireKind::He { "HE" } else { "SMOKE" }
            )),
            Tool::AdjustFire => Some("LMB ADJUST FIRE TO HERE · RMB / ESC CANCEL".to_owned()),
            Tool::BattlePosition { centre: None } => Some("LMB PLACE BP CENTRE · RMB CANCEL · [ ] RADIUS".to_owned()),
            Tool::BattlePosition { .. } => Some("LMB SET THREAT DIRECTION · RMB CANCEL · [ ] RADIUS".to_owned()),
            Tool::Viewshed => Some("LMB VIEWSHED FROM HERE · V / ESC CLOSE".to_owned()),
            Tool::Los { .. } => Some("LMB PLACE LOS POINT · ESC CLOSE".to_owned()),
            Tool::Graphic { kind: DrawKind::PhaseLine, .. } => {
                Some("LMB ADD POINT · ENTER FINISH · BKSP UNDO · RMB / ESC CANCEL".to_owned())
            }
            Tool::Graphic { .. } => Some("LMB PLACE · RMB / ESC CANCEL".to_owned()),
            Tool::PlanCp => {
                let k = self.plan.as_ref().map_or(1, |d| d.plan.steps.len() + 1);
                Some(format!("LMB ADD CP {k} · LMB DRAG CP MOVES IT · RMB CP MENU · ESC STOP ADDING"))
            }
            Tool::PlanPick { pick, .. } => Some(format!("LMB {} · RMB / ESC CANCEL", pick.prompt())),
            Tool::None => None,
        };
        if let Some(t) = tool {
            return t;
        }
        let cp_no = |id: u16| self.shown_plan().and_then(|(_, pl, ..)| pl.index_of(id)).map_or(0, |i| i + 1);
        let who = self.addressee().map(|to| self.addressee_cs(to));
        match self.menu_target(cam, inp.mouse) {
            MenuTarget::PlanCp(id) => {
                let k = cp_no(id);
                format!("LMB SELECT CP {k} · LMB DRAG MOVES CP {k} · RMB CP {k} MENU · DEL DELETES")
            }
            MenuTarget::PlanLeg(id) => {
                let k = cp_no(id);
                format!("RMB LEG TO CP {k} MENU: INSERT CP OR SET THE LEG · LMB SELECT")
            }
            MenuTarget::Unit(_) => "LMB SELECT · RMB UNIT MENU".into(),
            MenuTarget::Contact(_) => match who {
                Some(w) => format!("RMB CONTACT MENU · CTRL+RMB {w} ENGAGE"),
                None => "RMB CONTACT MENU".into(),
            },
            MenuTarget::Graphic(_) => "RMB GRAPHIC MENU: MOVE TO, CALL FIRE, DELETE".into(),
            MenuTarget::Ground => match who {
                None => "LMB SELECT · LMB DRAG BOX SELECT · RMB MENU".into(),
                Some(w) => {
                    let replaces = self.sel_platoon.is_some_and(|p| self.shown_plan().is_some_and(|s| s.0 == p));
                    let tail = if replaces { " (REPLACES PLAN)" } else { "" };
                    format!("RMB MOVE {w}{tail} · SHIFT+RMB ADD ROUTE POINT · HOLD RMB MENU · LMB SELECT")
                }
            },
        }
    }

    /// Draw the hint line just above the bottom band.
    pub(super) fn draw_hint(&self, f: &mut Frame, screen: P2) {
        if self.hint.is_empty() {
            return;
        }
        let y = hud::band_y(screen) - 22.0;
        let w = f.text_width(&self.hint, style::FONT_S) + 20.0;
        f.rect(0.0, y, w, 20.0, Rgba(style::PANEL.0, style::PANEL.1, style::PANEL.2, 215));
        f.text(&self.hint, 10.0, y + 3.0, style::FONT_S, style::TEXT);
    }
}
