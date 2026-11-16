use super::*;
#[derive(Default)]
pub(super) struct InlineSeries {
    pub id: String,
    pub seasons: Vec<Item>,
    pub selected: usize,
    pub episodes: Vec<Item>,
    pub episode_focus: usize,
    focus_id: Option<String>,
    total: usize,
    loading: bool,
    error: String,
}
impl Aster {
    pub(super) fn return_from_detail(&mut self) {
        if let Some(parent) = self.detail_parent.take() {
            self.detail = Some(parent);
            self.detail_focus = 4 + self.inline.seasons.len() + self.inline.episode_focus;
            self.detail_requested.clear();
        } else {
            self.detail = None;
            self.detail_focus = 0;
        }
    }

    pub(super) fn inline_playback_started(&mut self, item: &Item) {
        if item.series_id.as_deref() != Some(self.inline.id.as_str())
            || self.detail_parent.is_none()
        {
            return;
        }
        if let Some(index) = self.inline.episodes.iter().position(|i| i.id == item.id) {
            self.inline.episode_focus = index;
        }
        if let Some(season) = self
            .inline
            .seasons
            .iter()
            .position(|s| Some(s.id.as_str()) == item.season_id.as_deref())
            && season != self.inline.selected
        {
            self.inline.selected = season;
            self.inline.focus_id = Some(item.id.clone());
            self.fetch_inline_episodes(false);
        }
    }
    fn fetch_inline_episodes(&mut self, append: bool) {
        let Some(season) = self.inline.seasons.get(self.inline.selected) else {
            return;
        };
        let Some(api) = self.api.clone() else { return };
        let id = self.inline.id.clone();
        let season = season.id.clone();
        let offset = if append {
            self.inline.episodes.len()
        } else {
            0
        };
        if !append {
            self.inline.episodes.clear();
            self.inline.episode_focus = 0;
            self.inline.total = 0;
        }
        self.inline.loading = true;
        self.inline.error.clear();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = api.episodes(&id, Some(&season), offset);
            let _ = tx.send(Message::InlineEpisodes(id, season, append, result));
        });
    }
    pub(super) fn receive_inline_seasons(
        &mut self,
        id: String,
        result: Result<LibraryPage, String>,
    ) {
        if self.inline.id != id {
            return;
        }
        self.inline.loading = false;
        match result {
            Ok(page) => {
                self.inline.seasons = page.items;
                self.inline.selected = self
                    .inline
                    .seasons
                    .iter()
                    .position(|s| s.index_number != Some(0))
                    .unwrap_or(0);
                if self.detail_focus >= 4 {
                    self.detail_focus = 4 + self.inline.selected;
                }
                self.fetch_inline_episodes(false);
            }
            Err(e) => self.inline.error = e,
        }
    }
    pub(super) fn receive_inline_episodes(
        &mut self,
        id: String,
        season: String,
        append: bool,
        result: Result<LibraryPage, String>,
    ) {
        if self.inline.id != id
            || self
                .inline
                .seasons
                .get(self.inline.selected)
                .is_none_or(|s| s.id != season)
        {
            return;
        }
        self.inline.loading = false;
        match result {
            Ok(mut page) => {
                let show = self.detail.as_ref().filter(|i| i.id == id);
                for item in &mut page.items {
                    item.series_id.get_or_insert_with(|| id.clone());
                    if let Some(show) = show {
                        item.series_name.get_or_insert_with(|| show.name.clone());
                    }
                }
                self.inline.total = page.total_record_count;
                if append {
                    self.inline.episodes.extend(page.items);
                } else {
                    self.inline.episodes = page.items;
                }
                if let Some(id) = self.inline.focus_id.take()
                    && let Some(index) = self.inline.episodes.iter().position(|i| i.id == id)
                {
                    self.inline.episode_focus = index;
                }
            }
            Err(e) => self.inline.error = e,
        }
    }
    fn open_inline_episode(&mut self, index: usize) {
        if let Some(item) = self.inline.episodes.get(index).cloned() {
            self.inline.episode_focus = index;
            self.detail_parent = self.detail.clone();
            self.detail = Some(item);
            self.detail_focus = 0;
            self.detail_requested.clear();
        }
    }
    pub(super) fn inline_action(&mut self, a: Action) -> bool {
        if self.detail.as_ref().is_none_or(|i| i.kind != "Series") {
            return false;
        }
        let count = self.inline.seasons.len();
        if self.detail_focus < 4 {
            if a == Action::Down && count > 0 {
                self.detail_focus = 4 + self.inline.selected;
                return true;
            }
            return false;
        }
        let index = self.detail_focus - 4;
        if a == Action::Back {
            self.detail_focus = 0;
            return true;
        }
        if matches!(a, Action::Audio | Action::Subtitles) {
            return false;
        }
        if index < count {
            match a {
                Action::Left if index > 0 => self.detail_focus -= 1,
                Action::Right if index + 1 < count => self.detail_focus += 1,
                Action::Up => self.detail_focus = 0,
                Action::Down if !self.inline.episodes.is_empty() => {
                    self.detail_focus = 4
                        + count
                        + self
                            .inline
                            .episode_focus
                            .min(self.inline.episodes.len() - 1)
                }
                Action::Confirm => {
                    self.inline.selected = index;
                    self.fetch_inline_episodes(false);
                }
                _ => {}
            }
        } else {
            let episode = index - count;
            match a {
                Action::Left if episode > 0 => self.detail_focus -= 1,
                Action::Right if episode + 1 < self.inline.episodes.len() => self.detail_focus += 1,
                Action::Up => {
                    self.inline.episode_focus = episode;
                    self.detail_focus = 4 + self.inline.selected;
                }
                Action::Confirm => self.open_inline_episode(episode),
                _ => {}
            }
        }
        true
    }
    pub(super) fn draw_inline_series(&mut self, ui: &mut egui::Ui, show: &Item) {
        if self.inline.id != show.id {
            self.inline = InlineSeries {
                id: show.id.clone(),
                loading: true,
                ..Default::default()
            };
            if let Some(api) = self.api.clone() {
                let tx = self.tx.clone();
                let id = show.id.clone();
                thread::spawn(move || {
                    let result = Route::Seasons {
                        series: Box::new(Item {
                            id: id.clone(),
                            kind: "Series".into(),
                            ..Item::default()
                        }),
                    }
                    .fetch(&api, "Series", "", 0, browse::Options::default());
                    let _ = tx.send(Message::InlineSeasons(id, result));
                });
            }
        }
        ui.add_space(28.0);
        Self::shelf_title(ui, "Seasons");
        ui.add_space(14.0);
        let mut choose = None;
        let mut focused_rect = None;
        egui::ScrollArea::horizontal()
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .id_salt(("inline-seasons", &show.id))
            .show(ui, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    for (index, season) in self.inline.seasons.iter().enumerate() {
                        let selected = self.inline.selected == index;
                        let focused = self.detail_focus == 4 + index;
                        let (rect, r) =
                            ui.allocate_exact_size(Vec2::new(124.0, 42.0), egui::Sense::click());
                        let t = ui.ctx().animate_bool_with_time(
                            ui.id().with(("season-focus", index)),
                            focused || r.hovered(),
                            0.16,
                        );
                        ui.painter().rect_filled(
                            rect,
                            6.0,
                            Color32::from_rgba_unmultiplied(230, 185, 157, (t * 20.0) as u8),
                        );
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            season.season_label(),
                            egui::FontId::proportional(15.0),
                            if selected {
                                ACCENT
                            } else if focused {
                                design::TEXT
                            } else {
                                MUTED
                            },
                        );
                        if selected {
                            let line = egui::Rect::from_min_size(
                                egui::pos2(rect.left() + 22.0, rect.bottom() - 2.0),
                                Vec2::new(rect.width() - 44.0, 2.0),
                            );
                            ui.painter().rect_filled(line, 1.0, ACCENT);
                        }
                        if focused && self.focus_changed {
                            design::reveal_horizontal(ui, r.rect);
                            focused_rect = Some(r.rect);
                        }
                        if r.clicked() {
                            choose = Some(index);
                        }
                        ui.add_space(8.0);
                    }
                });
            });
        if let Some(rect) = focused_rect {
            design::reveal_vertical(ui, rect);
        }
        if let Some(index) = choose {
            self.inline.selected = index;
            self.detail_focus = 4 + index;
            self.fetch_inline_episodes(false);
        }
        ui.add_space(18.0);
        if self.inline.loading {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Loading episodes…");
            });
        }
        if !self.inline.error.is_empty() {
            ui.colored_label(Color32::LIGHT_RED, &self.inline.error);
            if ui.button("Retry").clicked() {
                self.inline.id.clear();
            }
        }
        let mut open = None;
        let count = self.inline.seasons.len();
        let mut focused_episode = None;
        egui::ScrollArea::horizontal()
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .id_salt(("inline-episodes", &show.id, self.inline.selected))
            .show(ui, |ui| {
                ui.add_space(4.0);
                ui.horizontal_top(|ui| {
                    ui.add_space(6.0);
                    for (index, item) in self.inline.episodes.iter().enumerate() {
                        let width = (ui.ctx().content_rect().width() / 4.6).clamp(220.0, 360.0);
                        let h = width * 9.0 / 16.0;
                        let (card, r) = ui
                            .allocate_exact_size(Vec2::new(width, h + 62.0), egui::Sense::click());
                        let art = egui::Rect::from_min_size(card.min, Vec2::new(width, h));
                        self.cinematic_image(ui, item, art, false, 9);
                        let focused = self.detail_focus == 4 + count + index;
                        design::focus_ring(
                            ui,
                            art,
                            ui.id().with(("episode-focus", index)),
                            focused || r.hovered(),
                            10.0,
                        );
                        let label = format!("{} · {}", item.episode_code(), item.name);
                        design::text(
                            ui,
                            egui::Rect::from_min_size(
                                card.min + Vec2::new(0.0, h + 10.0),
                                Vec2::new(width, 24.0),
                            ),
                            &label,
                            14.0,
                            design::TEXT,
                            1,
                        );
                        let meta = format!(
                            "{} min{}",
                            item.run_time_ticks.unwrap_or(0) / 600_000_000,
                            if item.user_data.played {
                                " · Watched"
                            } else if item.user_data.playback_position_ticks > 0 {
                                " · In progress"
                            } else {
                                ""
                            }
                        );
                        design::text(
                            ui,
                            egui::Rect::from_min_size(
                                card.min + Vec2::new(0.0, h + 36.0),
                                Vec2::new(width, 20.0),
                            ),
                            &meta,
                            12.0,
                            MUTED,
                            1,
                        );
                        if focused && self.focus_changed {
                            design::reveal_horizontal(ui, card);
                            focused_episode = Some(card);
                        }
                        if r.clicked() {
                            open = Some(index);
                        }
                        ui.add_space(14.0);
                    }
                });
            });
        if let Some(rect) = focused_episode {
            design::reveal_vertical(ui, rect);
        }
        if let Some(index) = open {
            self.open_inline_episode(index);
        }
        if self.inline.episodes.len() < self.inline.total
            && !self.inline.loading
            && ui.button("Load more episodes").clicked()
        {
            self.fetch_inline_episodes(true);
        }
        if !self.inline.loading && self.inline.episodes.is_empty() && self.inline.error.is_empty() {
            ui.label("No episodes available.");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Aster {
        let ctx = egui::Context::default();
        let mut app = Aster::new(&ctx, Settings::default());
        app.detail = Some(Item {
            id: "show".into(),
            kind: "Series".into(),
            name: "Show".into(),
            ..Item::default()
        });
        app.focus = 17;
        app.inline = InlineSeries {
            id: "show".into(),
            seasons: vec![
                Item {
                    id: "one".into(),
                    ..Item::default()
                },
                Item {
                    id: "two".into(),
                    ..Item::default()
                },
            ],
            episodes: vec![Item {
                id: "episode".into(),
                kind: "Episode".into(),
                ..Item::default()
            }],
            ..Default::default()
        };
        app
    }
    #[test]
    fn inline_seasons_episodes_and_back_preserve_the_series_page_and_library_focus() {
        let mut app = fixture();
        app.detail_focus = 0;
        assert!(app.inline_action(Action::Down));
        assert_eq!(app.detail_focus, 4);
        app.inline_action(Action::Down);
        assert_eq!(app.detail_focus, 6);
        app.inline_action(Action::Confirm);
        assert_eq!(app.detail.as_ref().unwrap().id, "episode");
        assert!(
            app.route.is_library(),
            "inline browsing must not navigate to a season catalog"
        );
        app.return_from_detail();
        assert_eq!(app.detail.as_ref().unwrap().id, "show");
        assert_eq!(app.detail_focus, 6);
        assert_eq!(app.focus, 17);
        app.inline_action(Action::Up);
        assert_eq!(app.detail_focus, 4);
        app.inline_action(Action::Back);
        assert_eq!(app.detail_focus, 0);
        app.return_from_detail();
        assert!(app.detail.is_none());
        assert_eq!(app.focus, 17);
    }
    #[test]
    fn late_episode_responses_cannot_replace_the_selected_season() {
        let mut app = fixture();
        app.inline.selected = 1;
        app.receive_inline_episodes(
            "show".into(),
            "one".into(),
            false,
            Ok(LibraryPage {
                items: vec![],
                total_record_count: 0,
            }),
        );
        assert_eq!(app.inline.episodes[0].id, "episode");
        app.receive_inline_episodes(
            "other-show".into(),
            "two".into(),
            false,
            Ok(LibraryPage {
                items: vec![],
                total_record_count: 0,
            }),
        );
        assert_eq!(app.inline.episodes.len(), 1);
    }
}
