//! Code-word strip (docs/plans/PLANS-AND-CONTEXT-MENU.md §4.4): the words the side's plans and
//! contingencies wait on, who is waiting and whether they are set, and the button that sends
//! each one (FM 6-0 execution by code word: one transmission releases every platoon at once).
//!
//! Readiness comes from the platoons' own reports in the picture, never from truth: a platoon is
//! SET once its plan is holding at the checkpoint that waits on the word (after its BP, if any,
//! is reported set).

use super::*;

/// Where a platoon is relative to the checkpoint that waits on a word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ready {
    /// Holding there (its BP, if any, is set).
    Set,
    /// On the way there, or setting into its BP.
    Moving,
    /// The wait is on a later checkpoint.
    Later,
}

/// One word in use: who waits on it and how many of them are set.
#[derive(Debug, Clone, PartialEq)]
pub struct WordStatus {
    pub word: u8,
    pub name: String,
    /// Each platoon whose plan waits on the word.
    pub platoons: Vec<(String, Ready)>,
    /// Contingency orders (triggers) waiting on it.
    pub triggers: usize,
}

impl WordStatus {
    /// Every waiting platoon is set (and at least one waits).
    pub fn all_set(&self) -> bool {
        !self.platoons.is_empty() && self.platoons.iter().all(|p| p.1 == Ready::Set)
    }

    pub fn text(&self) -> String {
        if self.all_set() {
            return "ALL SET".into();
        }
        let mut v: Vec<String> = self
            .platoons
            .iter()
            .map(|(cs, r)| {
                let r = match r {
                    Ready::Set => "SET",
                    Ready::Moving => "MOVING",
                    Ready::Later => "LATER",
                };
                format!("{cs} {r}")
            })
            .collect();
        if self.triggers > 0 {
            v.push(format!("{} CONTINGENCY", self.triggers));
        }
        v.join(" · ")
    }
}

const ROW_H: f32 = 22.0;

impl Game {
    /// Code words the side's sent plans and unfired contingencies wait on, in word order.
    pub(super) fn word_status(&self) -> Vec<WordStatus> {
        let pic = self.picture();
        let mut out: Vec<WordStatus> = Vec::new();
        let entry = |word: u8, out: &mut Vec<WordStatus>| -> usize {
            match out.iter().position(|w| w.word == word) {
                Some(i) => i,
                None => {
                    out.push(WordStatus { word, name: self.code_word_name(word), platoons: Vec::new(), triggers: 0 });
                    out.len() - 1
                }
            }
        };
        for v in &pic.platoons {
            let Some(pv) = &v.plan else { continue };
            let cursor = pv.plan.index_of(pv.cursor_id).unwrap_or(0);
            let mut seen: Vec<u8> = Vec::new();
            for (i, s) in pv.plan.steps.iter().enumerate().skip(cursor) {
                for c in s.wait.iter().flat_map(|w| w.conditions.iter()) {
                    if let TriggerCondition::CodeWord { word } = c
                        && !seen.contains(word)
                    {
                        seen.push(*word);
                        let set = match (i == cursor, pv.waiting.is_some()) {
                            (true, true) => Ready::Set,
                            (true, false) => Ready::Moving,
                            _ => Ready::Later,
                        };
                        let k = entry(*word, &mut out);
                        out[k].platoons.push((v.callsign.clone(), set));
                    }
                }
            }
        }
        for t in pic.triggers.iter().filter(|t| !t.fired) {
            if let TriggerCondition::CodeWord { word } = t.condition {
                let k = entry(word, &mut out);
                out[k].triggers += 1;
            }
        }
        out.sort_by_key(|w| w.word);
        out
    }

    /// Frame of the strip, under the roster and the fire support panel.
    pub(super) fn code_panel(&self, roster_rows: f32) -> Option<Rect> {
        let n = self.word_status().len() as f32;
        if n == 0.0 {
            return None;
        }
        let mut y = TOP_H + 8.0 + 24.0 + roster_rows * 24.0;
        if let Some(f) = self.fire_panel(roster_rows) {
            y = f.y + f.h + 4.0;
        }
        Some(Rect::new(4.0, y, ROSTER_W + 4.0, 26.0 + n * ROW_H))
    }

    /// One button per word in use: sends it (Ctrl+1..9 by its place in the doctrine list).
    pub(super) fn code_buttons(&self, roster_rows: f32) -> Vec<Button<GameAction>> {
        let Some(r) = self.code_panel(roster_rows) else { return Vec::new() };
        self.word_status()
            .into_iter()
            .enumerate()
            .map(|(k, w)| {
                let key = ["CTRL+1", "CTRL+2", "CTRL+3", "CTRL+4", "CTRL+5", "CTRL+6", "CTRL+7", "CTRL+8", "CTRL+9"]
                    .get(w.word as usize)
                    .copied()
                    .unwrap_or("");
                let rr = Rect::new(r.x + 4.0, r.y + 22.0 + k as f32 * ROW_H, r.w - 8.0, ROW_H - 2.0);
                Button::new(rr, Look::Row, w.name.clone(), GameAction::CodeWord(w.word))
                    .value(w.text())
                    .key(key)
                    .colour(if w.all_set() { style::AMBER } else { style::TEXT })
                    .enabled(!self.planning)
                    .tip("SEND THE CODE WORD: EVERY PLAN AND CONTINGENCY WAITING ON IT GOES AT ONCE")
            })
            .collect()
    }

    /// After the clock moved: a word whose platoons are now all set is announced, and stops
    /// the clock if the option is on.
    pub(super) fn check_ready(&mut self) {
        let now: Vec<(u8, String)> =
            self.word_status().into_iter().filter(|w| w.all_set()).map(|w| (w.word, w.name)).collect();
        for (word, name) in &now {
            if !self.ready_words.contains(word) {
                self.radio(format!("{name}: ALL SET."));
                if self.pause_on_ready && !self.planning {
                    self.paused = true;
                }
            }
        }
        self.ready_words = now.into_iter().map(|w| w.0).collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_text_lists_platoons_until_all_are_set() {
        let mut w = WordStatus {
            word: 0,
            name: "HAMMER".into(),
            platoons: vec![("1".into(), Ready::Set), ("3".into(), Ready::Moving), ("4".into(), Ready::Later)],
            triggers: 0,
        };
        assert_eq!(w.text(), "1 SET · 3 MOVING · 4 LATER");
        assert!(!w.all_set());
        w.platoons[1].1 = Ready::Set;
        w.platoons[2].1 = Ready::Set;
        assert_eq!(w.text(), "ALL SET");
        w.platoons.clear();
        w.triggers = 2;
        assert!(!w.all_set(), "a contingency alone is never 'set'");
        assert_eq!(w.text(), "2 CONTINGENCY");
    }
}
