use crate::input::Action;
#[derive(Clone, Debug, PartialEq)]
pub enum KeyAction {
    Character(char),
    Shift,
    Page,
    Space,
    Delete,
    Done,
}
pub struct Key {
    pub label: String,
    pub action: KeyAction,
    pub units: f32,
}
pub struct Keyboard {
    pub field: usize,
    pub text: String,
    pub selected: usize,
    pub upper: bool,
    pub symbols: bool,
    pub request_focus: bool,
}
impl Keyboard {
    pub fn new(field: usize, text: String) -> Self {
        Self {
            field,
            text,
            selected: 0,
            upper: false,
            symbols: false,
            request_focus: true,
        }
    }
    pub fn rows(&self) -> Vec<Vec<Key>> {
        let rows = if self.symbols {
            vec!["!@#$%^&*()", "-_=+[]{}\\|", ";:'\",.<>/?", "`~"]
        } else {
            vec!["1234567890", "qwertyuiop", "asdfghjkl", "zxcvbnm./:"]
        };
        let mut rows: Vec<Vec<Key>> = rows
            .into_iter()
            .map(|row| {
                row.chars()
                    .map(|c| {
                        let c = if self.upper {
                            c.to_ascii_uppercase()
                        } else {
                            c
                        };
                        Key {
                            label: c.to_string(),
                            action: KeyAction::Character(c),
                            units: 1.0,
                        }
                    })
                    .collect()
            })
            .collect();
        rows.push(vec![
            Key {
                label: if self.upper {
                    "SHIFT".into()
                } else {
                    "Shift".into()
                },
                action: KeyAction::Shift,
                units: 1.5,
            },
            Key {
                label: if self.symbols {
                    "ABC".into()
                } else {
                    "#+=".into()
                },
                action: KeyAction::Page,
                units: 1.5,
            },
            Key {
                label: "Space".into(),
                action: KeyAction::Space,
                units: 3.5,
            },
            Key {
                label: "Delete".into(),
                action: KeyAction::Delete,
                units: 1.5,
            },
            Key {
                label: "Done".into(),
                action: KeyAction::Done,
                units: 2.0,
            },
        ]);
        rows
    }
    pub fn done_index(&self) -> usize {
        self.rows()
            .iter()
            .flatten()
            .position(|key| key.action == KeyAction::Done)
            .unwrap()
    }
    pub fn navigate(&mut self, action: Action) {
        let rows = self.rows();
        let mut offset = 0;
        let mut row = 0;
        let mut col = 0;
        for (r, keys) in rows.iter().enumerate() {
            if self.selected < offset + keys.len() {
                row = r;
                col = self.selected - offset;
                break;
            }
            offset += keys.len();
        }
        let next_row = match action {
            Action::Up => row.saturating_sub(1),
            Action::Down => (row + 1).min(rows.len() - 1),
            _ => row,
        };
        if next_row == row {
            match action {
                Action::Left => self.selected = offset + col.saturating_sub(1),
                Action::Right => self.selected = offset + (col + 1).min(rows[row].len() - 1),
                _ => {}
            }
            return;
        }
        let centers = |keys: &[Key]| {
            let total: f32 = keys.iter().map(|k| k.units).sum();
            let mut x = (10.0 - total) / 2.0;
            keys.iter()
                .map(|k| {
                    let center = x + k.units / 2.0;
                    x += k.units;
                    center
                })
                .collect::<Vec<_>>()
        };
        let x = centers(&rows[row])[col];
        let next = centers(&rows[next_row])
            .into_iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| (a - x).abs().total_cmp(&(b - x).abs()))
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.selected = rows[..next_row].iter().map(Vec::len).sum::<usize>() + next;
    }
    pub fn delete(&mut self) {
        self.text.pop();
        self.request_focus = true;
    }
    pub fn page(&mut self) {
        self.symbols = !self.symbols;
        self.selected = 0;
    }
    pub fn press(&mut self) -> bool {
        let Some(action) = self
            .rows()
            .into_iter()
            .flatten()
            .nth(self.selected)
            .map(|key| key.action)
        else {
            return false;
        };
        match action {
            KeyAction::Character(c) => self.text.push(c),
            KeyAction::Shift => self.upper = !self.upper,
            KeyAction::Page => self.page(),
            KeyAction::Space => self.text.push(' '),
            KeyAction::Delete => {
                self.text.pop();
            }
            KeyAction::Done => return true,
        };
        self.request_focus = true;
        false
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pages_cover_every_printable_ascii_character() {
        let mut k = Keyboard::new(2, String::new());
        let mut chars = String::new();
        for page in [false, true] {
            k.symbols = page;
            for key in k.rows().into_iter().flatten() {
                if let KeyAction::Character(c) = key.action {
                    chars.push(c);
                    chars.push(c.to_ascii_uppercase());
                }
            }
        }
        for c in b'!'..=b'~' {
            assert!(chars.contains(c as char), "Missing {}", c as char);
        }
    }
    #[test]
    fn navigation_uses_qwerty_rows_and_nearest_center() {
        let mut k = Keyboard::new(0, String::new());
        k.selected = 10;
        k.navigate(Action::Left);
        assert_eq!(k.selected, 10);
        k.navigate(Action::Down);
        assert_eq!(k.selected, 20);
        k.navigate(Action::Right);
        assert_eq!(k.selected, 21);
        k.navigate(Action::Up);
        assert_eq!(k.selected, 11);
        k.selected = k.done_index();
        k.navigate(Action::Down);
        assert_eq!(k.selected, k.done_index());
    }
    #[test]
    fn typing_shift_symbols_and_delete_preserves_text() {
        let mut k = Keyboard::new(0, String::new());
        k.selected = 10;
        k.press();
        assert_eq!(k.text, "q");
        k.upper = true;
        k.press();
        assert_eq!(k.text, "qQ");
        k.page();
        k.press();
        assert_eq!(k.text, "qQ!");
        k.delete();
        assert_eq!(k.text, "qQ");
        k.selected = k.done_index();
        assert!(k.press());
    }
}
