//! Feedback and feedforward (docs/plans/archive/CONTROL-UX.md §5.6): the top-bar hold state, the hold
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
        let tool = match &self.tool {
            Tool::Move => Some("LMB MOVE HERE · RMB / ESC CANCEL".to_owned()),
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
            let kb = !matches!(self.tool, Tool::Viewshed);
            return if kb { format!("{t} · CTRL+ARROWS CURSOR · CTRL+SPACE PLACE") } else { t };
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
                    let k = self.append_index();
                    format!("RMB MOVE {w}{tail} · SHIFT+RMB ADD CP {k} · HOLD RMB MENU · LMB SELECT")
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

    /// Number the next appended checkpoint would get.
    pub(super) fn append_index(&self) -> usize {
        if self.sel_platoon.is_none() {
            return self.draft.len() + 1;
        }
        self.platoon_points().len() + 1
    }

    /// The selected platoon's current orders as points: the plan's checkpoints, else its route.
    fn platoon_points(&self) -> Vec<[f64; 2]> {
        if let Some((_, plan, ..)) = self.shown_plan() {
            return plan.steps.iter().map(|s| s.at).collect();
        }
        self.sel_platoon.and_then(|p| self.picture().platoon(p)).map_or(Vec::new(), |v| v.waypoints.clone())
    }

    /// The point a right click would order to, when the cursor is on plain ground with a
    /// selection and nothing else (tool, menu, hold) is using it.
    pub(super) fn compute_ghost(&self, inp: &Input, cam: &Camera, over_ui: bool) -> Option<[f64; 2]> {
        let idle = self.tool == Tool::None && self.menu.is_none() && self.rmb.is_none() && self.aar.is_none();
        if !idle || over_ui || self.addressee().is_none() || self.menu_target(cam, inp.mouse) != MenuTarget::Ground {
            return None;
        }
        Some(cam.to_world(inp.mouse))
    }

    /// Where the ghost order starts: the last checkpoint with Shift, else the selection.
    fn ghost_from(&self) -> Option<[f64; 2]> {
        if self.shift_held {
            if self.sel_platoon.is_some() {
                if let Some(at) = self.platoon_points().last() {
                    return Some(*at);
                }
            } else if let Some(at) = self.draft.last() {
                return Some(*at);
            }
        }
        self.selection_centre()
    }

    /// Faint line from the order's start to the cursor, with length, time and what it would do
    /// to the current orders.
    pub(super) fn draw_ghost(&self, f: &mut Frame, cam: &Camera) {
        let (Some(to), Some(from)) = (self.ghost, self.ghost_from()) else { return };
        let col = Rgba(style::ROUTE_DRAFT.0, style::ROUTE_DRAFT.1, style::ROUTE_DRAFT.2, 120);
        route_line(f, cam, &[from, to], col, true);
        let len = ((to[0] - from[0]).powi(2) + (to[1] - from[1]).powi(2)).sqrt();
        let mut text = format::distance(len);
        if let Some(t) = self.ghost_time(from, to) {
            text.push_str(&format!(" · ~{}:{:02}", t as u64 / 60, t as u64 % 60));
        }
        let replaces = !self.shift_held && self.sel_platoon.is_some() && !self.platoon_points().is_empty();
        if replaces {
            text.push_str(" · REPLACES PLAN");
        } else if self.shift_held {
            text.push_str(&format!(" · ADD CP {}", self.append_index()));
        }
        let q = cam.to_screen(to);
        let w = f.text_width(&text, style::FONT_S) + 8.0;
        f.rect(q[0] + 14.0, q[1] + 10.0, w, 14.0, style::PANEL);
        f.text(&text, q[0] + 18.0, q[1] + 12.0, style::FONT_S, if replaces { style::AMBER } else { style::TEXT_DIM });
    }

    /// Straight-line travel time of the selection's lead vehicle (an estimate: no pathfinding).
    fn ghost_time(&self, from: [f64; 2], to: [f64; 2]) -> Option<f64> {
        let pic = self.picture();
        let id = match (self.sel_platoon, self.sel_unit) {
            (Some(p), _) => *pic.platoon(p)?.members.first()?,
            (None, u) => u?,
        };
        let u = pic.unit(id)?;
        let map = self.sim.map();
        let layer = map.mobility(&u.vehicle_id)?;
        let rt = terrain::path::segment_time(&layer.speed, map.manifest.cell_size_m, from, to)?;
        Some(rt)
    }

    /// DRAFT strip above the command card: SEND / REVERT / UNDO for the selected platoon's draft.
    pub(super) fn strip_buttons(&self, screen: P2) -> Vec<Button<GameAction>> {
        let Some(d) = &self.plan else { return Vec::new() };
        let z = Rect::new(0.0, 0.0, 0.0, 0.0);
        let b = |label: &str, c: Cmd| Button::new(z, Look::Bar, label, GameAction::Cmd(c));
        let n = d.plan.steps.len();
        let mut items = vec![
            b("SEND", Cmd::Send)
                .key("ENTER")
                .enabled(n > 0)
                .colour(style::AMBER)
                .tip("SEND THE PLAN TO THE PLATOON (ONE RADIO ORDER)"),
            b(if self.revert_armed { "REVERT: CONFIRM" } else { "REVERT" }, Cmd::PlanRevert)
                .active(self.revert_armed)
                .tip("DROP THE WHOLE DRAFT (THE PLATOON KEEPS ITS ORDERS); ASKS AGAIN AFTER SEVERAL EDITS. ESC NEVER DROPS A DRAFT"),
            b("UNDO", Cmd::Undo).key("CTRL+Z").enabled(d.changes() > 0).tip("UNDO THE LAST DRAFT EDIT"),
        ];
        if d.can_redo() {
            items.push(b("REDO", Cmd::Redo).key("CTRL+Y").tip("REDO THE EDIT UNDONE LAST"));
        }
        let total: f32 = hud::bar_width(&items) + 4.0 * items.len() as f32;
        let right = hud::card_frame(screen);
        let mut out = Vec::new();
        let x = right.x + right.w - total;
        hud::bar_row(&mut out, x, items);
        let y = hud::band_y(screen) - 26.0;
        for b in &mut out {
            b.r.y = y;
        }
        out
    }

    pub(super) fn draw_strip_text(&self, f: &mut Frame, screen: P2) {
        let Some(d) = &self.plan else { return };
        let Some(first) = self.strip_buttons(screen).first().map(|b| b.r) else { return };
        let k = d.changes();
        let text = format!("DRAFT · {k} CHANGE{}", if k == 1 { "" } else { "S" });
        let w = f.text_width(&text, style::FONT_S) + 16.0;
        f.rect(first.x - w - 8.0, first.y, w, first.h, style::PANEL);
        f.text(&text, first.x - w, first.y + 6.0, style::FONT_S, style::AMBER);
    }
}
