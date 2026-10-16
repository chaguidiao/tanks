//! Posture orders (docs/plans/archive/CONTROL-UX.md §5.3, §7): FORMATION, TECHNIQUE, SPEED, WEAPONS,
//! ON CONTACT and AMMO. Each is a value list: one pick is one radio order. With a checkpoint selected the pick
//! edits that leg of the plan, otherwise it goes to the selection now (and the sim writes it into
//! the leg in progress, see `sim::plan::PlanState::rewrite_leg`).

use super::ctx::{MenuAct, reaction_name};
use super::planedit::{PlanEdit, reaction_short};
use super::*;
use crate::hud::{MenuItem, PopupMenu};
use ai::picture::{ContactReaction, SpeedSetting};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Posture {
    Formation,
    Technique,
    Speed,
    Weapons,
    OnContact,
    Ammo,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PostureVal {
    Formation(Formation),
    Technique(Technique),
    Speed(Option<f64>),
    Weapons(WeaponsStatus),
    OnContact(ContactReaction),
    Ammo(Option<AmmoKind>),
    /// Leg scope: the leg does not set this; the platoon keeps what it has.
    Keep(Posture),
}

/// What a pick applies to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Scope {
    Now(Addressee),
    Leg(PlatoonId, u16),
}

const DIGITS: [&str; 9] = ["1", "2", "3", "4", "5", "6", "7", "8", "9"];

impl Posture {
    pub fn name(self) -> &'static str {
        match self {
            Posture::Formation => "FORMATION",
            Posture::Technique => "TECHNIQUE",
            Posture::Speed => "SPEED",
            Posture::Weapons => "WEAPONS",
            Posture::OnContact => "ON CONTACT",
            Posture::Ammo => "AMMUNITION",
        }
    }

    /// Command card cell of the button (the list opens at its corner).
    fn cell(self) -> usize {
        match self {
            Posture::Formation => 8,
            Posture::Technique => 9,
            Posture::Speed => 10,
            Posture::Weapons => 11,
            Posture::OnContact => 12,
            Posture::Ammo => 13,
        }
    }

    fn needs_platoon(self) -> bool {
        matches!(self, Posture::Formation | Posture::Technique | Posture::OnContact)
    }
}

impl Game {
    pub(super) fn request_posture(&mut self, k: Posture) {
        if self.posture_scope(k).is_some() {
            self.posture_request = Some(k);
        } else if k.needs_platoon() {
            self.radio("NO PLATOON SELECTED.".into());
        } else {
            self.radio("NO UNIT SELECTED.".into());
        }
    }

    /// The selected checkpoint's leg when one is selected (and the posture is part of a leg),
    /// else the selection now.
    pub(super) fn posture_scope(&self, k: Posture) -> Option<Scope> {
        if k.needs_platoon() && self.sel_platoon.is_none() {
            return None;
        }
        if k != Posture::Ammo
            && let Some(p) = self.sel_platoon
            && let Some(id) = self.selected_cp()
            && self.shown_plan().is_some_and(|(pp, pl, ..)| pp == p && pl.index_of(id).is_some())
        {
            return Some(Scope::Leg(p, id));
        }
        self.addressee().map(Scope::Now)
    }

    fn leg_settings(&self, p: PlatoonId, id: u16) -> Option<sim::view::LegSettings> {
        let (pp, pl, ..) = self.shown_plan()?;
        if pp != p {
            return None;
        }
        Some(pl.steps[pl.index_of(id)?].leg.clone())
    }

    fn posture_title(&self, k: Posture, scope: Scope) -> String {
        match scope {
            Scope::Now(to) => {
                let busy =
                    matches!(to, Addressee::Platoon(p) if self.picture().platoon(p).is_some_and(|v| v.plan.is_some()));
                format!("{} · {} · NOW{}", k.name(), self.addressee_cs(to), if busy { " (THIS LEG)" } else { "" })
            }
            Scope::Leg(p, id) => {
                let n = self.shown_plan().and_then(|(_, pl, ..)| pl.index_of(id)).map_or(0, |i| i + 1);
                let state = if self.plan.is_some() { "DRAFT" } else { "PLAN" };
                format!("{} · {} · LEG TO CP {n} ({state})", k.name(), self.platoon_cs(p))
            }
        }
    }

    /// The value the scope currently has (None = a leg that keeps the platoon's setting).
    fn posture_current(&self, k: Posture, scope: Scope) -> Option<PostureVal> {
        match scope {
            Scope::Now(to) => match k {
                Posture::Formation => {
                    let Addressee::Platoon(p) = to else { return None };
                    self.shown_formation(p).map(PostureVal::Formation)
                }
                Posture::Technique => {
                    let Addressee::Platoon(p) = to else { return None };
                    self.shown_technique(p).map(PostureVal::Technique)
                }
                Posture::Speed => self.shown_speed(to).map(PostureVal::Speed),
                Posture::Weapons => self.shown_weapons(to).map(PostureVal::Weapons),
                Posture::OnContact => {
                    let Addressee::Platoon(p) = to else { return None };
                    self.picture().platoon(p).map(|v| PostureVal::OnContact(v.contact_reaction))
                }
                Posture::Ammo => self.ammo_choice(to).map(|(c, _)| PostureVal::Ammo(c)),
            },
            Scope::Leg(p, id) => {
                let leg = self.leg_settings(p, id)?;
                match k {
                    Posture::Formation => leg.formation.map(PostureVal::Formation),
                    Posture::Technique => leg.technique.map(PostureVal::Technique),
                    Posture::Speed => leg.speed.map(|s| {
                        PostureVal::Speed(match s {
                            SpeedSetting::Max => None,
                            SpeedSetting::Limit(v) => Some(v),
                        })
                    }),
                    Posture::Weapons => leg.weapons.map(PostureVal::Weapons),
                    Posture::OnContact => leg.on_contact.map(PostureVal::OnContact),
                    Posture::Ammo => None,
                }
            }
        }
    }

    fn posture_label(&self, v: PostureVal) -> String {
        match v {
            PostureVal::Formation(f) => format_formation(f),
            PostureVal::Technique(t) => format_technique(t).into(),
            PostureVal::Speed(s) => speed_short(s),
            PostureVal::Weapons(w) => weapons_str(w).into(),
            PostureVal::OnContact(r) => reaction_name(r).into(),
            PostureVal::Ammo(k) => k.map_or("AUTO", |k| k.label()).into(),
            PostureVal::Keep(_) => "KEEP".into(),
        }
    }

    /// The text of the card button / menu value for this posture.
    pub(super) fn posture_value(&self, k: Posture) -> String {
        let Some(scope) = self.posture_scope(k) else { return String::new() };
        match self.posture_current(k, scope) {
            Some(v) => match (k, v) {
                (Posture::Technique, PostureVal::Technique(t)) => short_technique(t).into(),
                (Posture::OnContact, PostureVal::OnContact(r)) => reaction_short(r).into(),
                _ => self.posture_label(v),
            },
            None if matches!(scope, Scope::Leg(..)) => "KEEP".into(),
            None => String::new(),
        }
    }

    fn posture_vals(&self, k: Posture, scope: Scope) -> Vec<PostureVal> {
        match k {
            Posture::Formation => Formation::ALL.into_iter().map(PostureVal::Formation).collect(),
            Posture::Technique => [Technique::Traveling, Technique::TravelingOverwatch, Technique::BoundingOverwatch]
                .into_iter()
                .map(PostureVal::Technique)
                .collect(),
            Posture::Speed => self.speed_presets().into_iter().map(PostureVal::Speed).collect(),
            Posture::Weapons => [WeaponsStatus::Free, WeaponsStatus::Tight, WeaponsStatus::Hold]
                .into_iter()
                .map(PostureVal::Weapons)
                .collect(),
            Posture::OnContact => [
                ContactReaction::Drill,
                ContactReaction::Continue,
                ContactReaction::HastyPosition,
                ContactReaction::BreakContact,
            ]
            .into_iter()
            .map(PostureVal::OnContact)
            .collect(),
            Posture::Ammo => {
                let kinds = match scope {
                    Scope::Now(to) => self.ammo_choice(to).map(|(_, k)| k).unwrap_or_default(),
                    Scope::Leg(..) => Vec::new(),
                };
                std::iter::once(PostureVal::Ammo(None))
                    .chain(kinds.into_iter().map(|k| PostureVal::Ammo(Some(k))))
                    .collect()
            }
        }
    }

    pub(super) fn posture_items(&self, k: Posture, scope: Scope) -> Vec<MenuItem<MenuAct>> {
        let cur = self.posture_current(k, scope);
        let mut vals = self.posture_vals(k, scope);
        if matches!(scope, Scope::Leg(..)) {
            vals.insert(0, PostureVal::Keep(k));
        }
        let mut digit = 0;
        vals.into_iter()
            .map(|v| {
                let mut it = MenuItem::new(self.posture_label(v), MenuAct::Posture(scope, v));
                let keep = matches!(v, PostureVal::Keep(_));
                if !keep && digit < DIGITS.len() {
                    it = it.key(DIGITS[digit]);
                    digit += 1;
                }
                it.checked(if keep { cur.is_none() } else { cur == Some(v) })
            })
            .collect()
    }

    /// The posture as a submenu of the platoon context menu.
    pub(super) fn posture_item(&self, k: Posture, key: &'static str) -> MenuItem<MenuAct> {
        let Some(scope) = self.posture_scope(k) else {
            let why = if k.needs_platoon() { "NO PLATOON SELECTED" } else { "NO UNIT SELECTED" };
            return MenuItem::submenu(k.name(), Vec::new()).key(key).disabled(why);
        };
        MenuItem::submenu(k.name(), self.posture_items(k, scope)).key(key).value(self.posture_value(k))
    }

    pub(super) fn open_posture_menu(&mut self, inp: &Input) {
        let Some(k) = self.posture_request.take() else { return };
        let Some(scope) = self.posture_scope(k) else { return };
        let r = hud::card_cell(inp.screen, k.cell());
        self.menu = Some(PopupMenu::new([r.x, r.y], self.posture_title(k, scope), self.posture_items(k, scope)));
        self.menu_world = [0.0, 0.0];
    }

    pub(super) fn open_graphics_menu(&mut self, inp: &Input) {
        if !std::mem::take(&mut self.graphics_request) {
            return;
        }
        let gfx = |k: DrawKind| matches!(&self.tool, Tool::Graphic { kind, .. } if *kind == k);
        let items = vec![
            MenuItem::new("PHASE LINE", MenuAct::Cmd(Cmd::PhaseLine)).key("P").checked(gfx(DrawKind::PhaseLine)),
            MenuItem::new("OBJECTIVE", MenuAct::Cmd(Cmd::Objective)).key("O").checked(gfx(DrawKind::Objective)),
            MenuItem::new("TRP", MenuAct::Cmd(Cmd::Trp)).key("T").checked(gfx(DrawKind::Trp)),
        ];
        let r = hud::card_cell(inp.screen, 20);
        self.menu = Some(PopupMenu::new([r.x, r.y], "GRAPHICS · CONTROL MEASURES", items));
        self.menu_world = [0.0, 0.0];
    }

    pub(super) fn apply_posture(&mut self, scope: Scope, val: PostureVal) {
        match scope {
            Scope::Leg(_, id) => {
                let e = match val {
                    PostureVal::Formation(f) => PlanEdit::Formation(id, Some(f)),
                    PostureVal::Technique(t) => PlanEdit::Technique(id, Some(t)),
                    PostureVal::Speed(s) => PlanEdit::Speed(id, Some(s.map_or(SpeedSetting::Max, SpeedSetting::Limit))),
                    PostureVal::Weapons(w) => PlanEdit::Weapons(id, Some(w)),
                    PostureVal::OnContact(r) => PlanEdit::OnContact(id, Some(r)),
                    PostureVal::Keep(Posture::Formation) => PlanEdit::Formation(id, None),
                    PostureVal::Keep(Posture::Technique) => PlanEdit::Technique(id, None),
                    PostureVal::Keep(Posture::Speed) => PlanEdit::Speed(id, None),
                    PostureVal::Keep(Posture::Weapons) => PlanEdit::Weapons(id, None),
                    PostureVal::Keep(Posture::OnContact) => PlanEdit::OnContact(id, None),
                    PostureVal::Ammo(_) | PostureVal::Keep(Posture::Ammo) => return,
                };
                self.run_plan_edit(e, [0.0, 0.0]);
            }
            Scope::Now(to) => self.apply_now(to, val),
        }
    }

    fn apply_now(&mut self, to: Addressee, val: PostureVal) {
        let cs = self.addressee_cs(to);
        let track = self.order_mode == OrderMode::Now;
        let now = self.sim.time_s();
        match (val, to) {
            (PostureVal::Formation(f), Addressee::Platoon(platoon)) => {
                if track {
                    self.pending_formation.retain(|x| x.0 != platoon);
                    self.pending_formation.push((platoon, f, now));
                }
                self.order(Command::SetFormation { platoon, formation: f });
                self.radio(format!("TO {cs}: CHANGE FORMATION, {}.", format_formation(f)));
            }
            (PostureVal::Technique(technique), Addressee::Platoon(platoon)) => {
                if track {
                    self.pending_technique.retain(|x| x.0 != platoon);
                    self.pending_technique.push((platoon, technique, now));
                }
                self.order(Command::SetTechnique { platoon, technique });
                self.radio(format!("TO {cs}: TECHNIQUE {}.", format_technique(technique)));
            }
            (PostureVal::Speed(speed_limit_mps), _) => {
                if track {
                    self.pending_speed.retain(|x| x.0 != to);
                    self.pending_speed.push((to, speed_limit_mps, now));
                }
                self.order(Command::SetSpeed { to, speed_limit_mps });
                let sp = speed_limit_mps.map_or("MAX".into(), |v| format!("{} KM/H", format::kmh(v)));
                self.radio(format!("TO {cs}: SPEED {sp}."));
            }
            (PostureVal::Weapons(status), _) => {
                if track {
                    self.pending_weapons.retain(|x| x.0 != to);
                    self.pending_weapons.push((to, status, now));
                }
                self.order(Command::WeaponsControl { to, status });
                self.radio(format!("TO {cs}: WEAPONS {}.", weapons_str(status)));
            }
            (PostureVal::OnContact(reaction), Addressee::Platoon(platoon)) => {
                self.order(Command::SetContactReaction { platoon, reaction });
                self.radio(format!("TO {cs}: ON CONTACT, {}.", reaction_name(reaction)));
            }
            (PostureVal::Ammo(kind), _) => {
                self.order(Command::SelectAmmo { to, kind });
                self.radio(format!("TO {cs}: AMMUNITION {}.", kind.map_or("AUTO", |k| k.label())));
            }
            _ => {}
        }
    }

    pub(super) fn shown_technique(&self, p: PlatoonId) -> Option<Technique> {
        if let Some(t) = self.pending_technique.iter().find(|x| x.0 == p) {
            return Some(t.1);
        }
        self.picture().platoon(p).map(|v| v.technique)
    }

    /// The march speed limit of the addressee (outer None: unknown), or as just ordered.
    pub(super) fn shown_speed(&self, to: Addressee) -> Option<Option<f64>> {
        if let Some(s) = self.pending_speed.iter().find(|x| x.0 == to) {
            return Some(s.1);
        }
        self.actual_speed(to)
    }

    pub(super) fn actual_speed(&self, to: Addressee) -> Option<Option<f64>> {
        let pic = self.picture();
        match to {
            Addressee::Unit(u) => pic.unit(u).map(|u| u.speed_limit),
            Addressee::Platoon(p) => pic.platoon(p).map(|v| v.speed_limit),
        }
    }

    /// Weapons status of `to`, or as just ordered if not yet acknowledged.
    pub(super) fn shown_weapons(&self, to: Addressee) -> Option<WeaponsStatus> {
        if let Some(w) = self.pending_weapons.iter().find(|x| x.0 == to) {
            return Some(w.1);
        }
        self.actual_weapons(to)
    }

    /// The selection's march speed: what a new move inherits.
    pub(super) fn current_speed(&self) -> Option<f64> {
        self.addressee().and_then(|to| self.shown_speed(to)).flatten()
    }
}
