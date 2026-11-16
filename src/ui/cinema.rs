use super::*;
#[derive(Clone)]
pub(super) struct Shelf {
    pub title: String,
    pub landscape: bool,
    pub start: usize,
    pub count: usize,
}
impl Aster {
    pub(super) fn is_dashboard(&self) -> bool {
        self.tab == 0 && self.route.is_library() && self.search.is_empty()
    }
    fn featured_items(&self) -> Vec<Item> {
        let mut items = Vec::new();
        for item in self
            .shelves
            .iter()
            .filter(|s| !s.landscape)
            .flat_map(|s| self.items.get(s.start..s.start + s.count).unwrap_or(&[]))
        {
            if !items.iter().any(|i: &Item| i.id == item.id) {
                items.push(item.clone());
            }
            if items.len() == 6 {
                break;
            }
        }
        if items.is_empty() {
            items.extend(self.items.iter().take(6).cloned());
        }
        items
    }
    pub(super) fn dashboard_action(&mut self, a: Action) {
        if self.focus == 6 + self.items.len() {
            let featured = self.featured_items();
            match a {
                Action::Left => self.feature_index = self.feature_index.saturating_sub(1),
                Action::Right => {
                    self.feature_index =
                        (self.feature_index + 1).min(featured.len().saturating_sub(1))
                }
                Action::Down => self.focus = 6,
                Action::Up => self.focus = self.tab,
                Action::Confirm => {
                    self.detail = featured.get(self.feature_index).cloned();
                    self.detail_focus = 0;
                }
                _ => {}
            }
            return;
        }
        let index = self.focus.saturating_sub(6);
        let Some(row) = self
            .shelves
            .iter()
            .position(|s| index >= s.start && index < s.start + s.count)
        else {
            return;
        };
        let shelf = &self.shelves[row];
        let column = index - shelf.start;
        match a {
            Action::Left if column > 0 => self.focus -= 1,
            Action::Right if column + 1 < shelf.count => self.focus += 1,
            Action::Up => {
                self.focus = if row == 0 {
                    self.items.len() + 6
                } else {
                    let s = &self.shelves[row - 1];
                    6 + s.start + column.min(s.count - 1)
                }
            }
            Action::Down if row + 1 < self.shelves.len() => {
                let s = &self.shelves[row + 1];
                self.focus = 6 + s.start + column.min(s.count - 1)
            }
            Action::Confirm => self.activate_item(index),
            _ => {}
        }
    }
    pub(super) fn cinematic_image(
        &self,
        ui: &egui::Ui,
        item: &Item,
        rect: egui::Rect,
        backdrop: bool,
        radius: u8,
    ) {
        if !ui.is_rect_visible(rect) {
            return;
        }
        ui.painter().rect_filled(rect, radius, design::SURFACE);
        if let Some((image, uv)) = self.artwork(item, backdrop) {
            design::cover(ui, image, rect, uv, radius);
        }
    }
    fn cinematic_fade(ui: &egui::Ui, rect: egui::Rect) {
        design::fade(
            ui,
            rect,
            &[
                (0.0, Color32::from_black_alpha(0)),
                (0.35, Color32::from_black_alpha(10)),
                (0.72, Color32::from_black_alpha(130)),
                (1.0, BG),
            ],
            true,
        );
        design::fade(
            ui,
            rect,
            &[
                (0.0, Color32::from_black_alpha(95)),
                (0.45, Color32::TRANSPARENT),
                (1.0, Color32::TRANSPARENT),
            ],
            false,
        );
    }
    fn round_action(
        &mut self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        label: &str,
        index: usize,
        primary: bool,
    ) {
        let response = ui.interact(
            rect,
            ui.id().with(("detail-action", index)),
            egui::Sense::click(),
        );
        let focused = self.detail_focus == index;
        ui.painter().rect_filled(
            rect,
            rect.height() / 2.0,
            if primary {
                ACCENT
            } else {
                Color32::from_black_alpha(160)
            },
        );
        design::focus_ring(
            ui,
            rect,
            ui.id().with(("detail-glow", index)),
            focused || response.hovered(),
            rect.height() / 2.0,
        );
        if index == 3 {
            let c = rect.center();
            ui.painter().line_segment(
                [c + Vec2::new(7.0, 0.0), c - Vec2::new(7.0, 0.0)],
                Stroke::new(2.0_f32, design::TEXT),
            );
            ui.painter().line_segment(
                [c - Vec2::new(7.0, 0.0), c + Vec2::new(-1.0, -6.0)],
                Stroke::new(2.0_f32, design::TEXT),
            );
            ui.painter().line_segment(
                [c - Vec2::new(7.0, 0.0), c + Vec2::new(-1.0, 6.0)],
                Stroke::new(2.0_f32, design::TEXT),
            );
        } else {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(if primary { 17.0 } else { 15.0 }),
                if primary { BG } else { design::TEXT },
            );
        }
        if focused && self.focus_changed {
            response.scroll_to_me(None);
        }
        if response.clicked() {
            self.detail_focus = index;
            self.detail_activate();
        }
    }
    pub(super) fn draw_cinematic_detail(&mut self, ui: &mut egui::Ui) {
        let Some(item) = self.detail.clone() else {
            return;
        };
        if self.detail_requested != item.id {
            self.detail_requested = item.id.clone();
            if self.preview.is_none()
                && let Some(api) = self.api.clone()
            {
                let id = item.id.clone();
                let tx = self.tx.clone();
                let ctx = ui.ctx().clone();
                thread::spawn(move || {
                    let result = api.item_details(&id).map(Box::new);
                    let _ = tx.send(Message::Details(id, result));
                    ctx.request_repaint();
                });
            }
        }
        let width = ui.available_width();
        let height = (ui.ctx().content_rect().height() * 0.56).clamp(330.0, 650.0);
        let (hero, _) = ui.allocate_exact_size(Vec2::new(width, height), egui::Sense::hover());
        self.hero_bounds = Some(hero);
        self.cinematic_image(ui, &item, hero, true, 0);
        Self::cinematic_fade(ui, hero);
        let pad = 24.0;
        let poster_w = (height * 0.24).clamp(86.0, 140.0);
        let poster = egui::Rect::from_min_size(
            egui::pos2(hero.left() + pad, hero.bottom() - poster_w * 1.5 - 28.0),
            Vec2::new(poster_w, poster_w * 1.5),
        );
        let mut poster_item = item.clone();
        if item.kind == "Episode"
            && let Some(id) = &item.series_id
        {
            poster_item.id = id.clone();
            poster_item.kind = "Series".into();
        }
        self.cinematic_image(ui, &poster_item, poster, false, 8);
        let left = poster.right() + 24.0;
        let title_width = (width - poster_w - 320.0).max(210.0);
        let title = if item.kind == "Episode" {
            item.series_name.as_deref().unwrap_or(&item.name)
        } else {
            &item.name
        };
        design::text(
            ui,
            egui::Rect::from_min_size(
                egui::pos2(left, poster.bottom() - 108.0),
                Vec2::new(title_width, 54.0),
            ),
            title,
            if width > 1100.0 { 36.0 } else { 28.0 },
            design::TEXT,
            1,
        );
        let subtitle = if item.kind == "Episode" {
            format!("{}  ·  {}", item.episode_code(), item.name)
        } else {
            metadata(&item)
        };
        design::text(
            ui,
            egui::Rect::from_min_size(
                egui::pos2(left, poster.bottom() - 53.0),
                Vec2::new(title_width, 24.0),
            ),
            &subtitle,
            15.0,
            MUTED,
            1,
        );
        if let Some(rating) = item.community_rating {
            design::text(
                ui,
                egui::Rect::from_min_size(
                    egui::pos2(left, poster.bottom() - 22.0),
                    Vec2::new(110.0, 22.0),
                ),
                &format!("★  {rating:.1}"),
                15.0,
                ACCENT,
                1,
            );
        }
        let primary = if item.kind == "Series" {
            "Seasons"
        } else if self.settings.resume && item.user_data.playback_position_ticks > 0 {
            "▶ Resume"
        } else {
            "▶ Play"
        };
        let y = hero.bottom() - 76.0;
        let right = hero.right() - 24.0;
        self.round_action(
            ui,
            egui::Rect::from_min_size(egui::pos2(right - 254.0, y), Vec2::new(126.0, 48.0)),
            primary,
            0,
            true,
        );
        self.round_action(
            ui,
            egui::Rect::from_min_size(egui::pos2(right - 112.0, y), Vec2::splat(48.0)),
            if item.user_data.is_favorite {
                "♥"
            } else {
                "♡"
            },
            1,
            false,
        );
        self.round_action(
            ui,
            egui::Rect::from_min_size(egui::pos2(right - 48.0, y), Vec2::splat(48.0)),
            if item.user_data.played { "✓" } else { "○" },
            2,
            false,
        );
        self.round_action(
            ui,
            egui::Rect::from_min_size(hero.min + Vec2::splat(20.0), Vec2::splat(40.0)),
            "←",
            3,
            false,
        );
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(24, 0))
            .show(ui, |ui| {
                ui.add_space(18.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(if item.overview.is_empty() {
                            "No synopsis available."
                        } else {
                            &item.overview
                        })
                        .size(16.0)
                        .color(design::TEXT),
                    )
                    .wrap(),
                );
                ui.add_space(14.0);
                ui.horizontal_wrapped(|ui| {
                    for genre in &item.genres {
                        egui::Frame::new()
                            .fill(Color32::from_rgb(21, 23, 27))
                            .stroke(Stroke::new(1.0_f32, Color32::from_gray(50)))
                            .corner_radius(6)
                            .inner_margin(egui::Margin::symmetric(12, 6))
                            .show(ui, |ui| {
                                ui.label(RichText::new(genre).size(13.0).color(design::TEXT));
                            });
                    }
                });
                if item.kind == "Series" {
                    self.draw_inline_series(ui, &item);
                }
                if !item.people.is_empty() {
                    ui.add_space(26.0);
                    Self::shelf_title(ui, "Cast & Crew");
                    ui.add_space(14.0);
                    egui::ScrollArea::horizontal()
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                        .id_salt(("cast", &item.id))
                        .show(ui, |ui| {
                            ui.add_space(4.0);
                            ui.horizontal_top(|ui| {
                                ui.add_space(6.0);
                                for person in item.people.iter().take(30) {
                                    ui.allocate_ui_with_layout(
                                        Vec2::new(100.0, 145.0),
                                        egui::Layout::top_down(egui::Align::Center),
                                        |ui| {
                                            let (portrait, _) = ui.allocate_exact_size(
                                                Vec2::splat(82.0),
                                                egui::Sense::hover(),
                                            );
                                            ui.painter().circle_filled(
                                                portrait.center(),
                                                41.0,
                                                design::SURFACE,
                                            );
                                            if person.primary_image_tag.is_some() {
                                                if let Some(api) = &self.api {
                                                    design::cover(
                                                        ui,
                                                        egui::Image::new(api.portrait(&person.id)),
                                                        portrait,
                                                        egui::Rect::from_min_max(
                                                            egui::Pos2::ZERO,
                                                            egui::pos2(1.0, 1.0),
                                                        ),
                                                        41,
                                                    );
                                                }
                                            } else {
                                                ui.painter().text(
                                                    portrait.center(),
                                                    egui::Align2::CENTER_CENTER,
                                                    person.name.chars().next().unwrap_or('?'),
                                                    egui::FontId::proportional(28.0),
                                                    MUTED,
                                                );
                                            }
                                            ui.add_space(8.0);
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(&person.name).size(13.0),
                                                )
                                                .wrap(),
                                            );
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(
                                                        person
                                                            .role
                                                            .as_deref()
                                                            .unwrap_or(&person.kind),
                                                    )
                                                    .size(11.0)
                                                    .color(MUTED),
                                                )
                                                .truncate(),
                                            );
                                        },
                                    );
                                    ui.add_space(12.0);
                                }
                            });
                        });
                }
                ui.add_space(36.0);
            });
    }
    pub(super) fn shelf_title(ui: &mut egui::Ui, title: &str) {
        ui.horizontal(|ui| {
            let (bar, _) = ui.allocate_exact_size(Vec2::new(3.0, 19.0), egui::Sense::hover());
            ui.painter().rect_filled(bar, 2.0, ACCENT);
            ui.label(RichText::new(title).size(21.0).strong().color(design::TEXT));
        });
    }
    pub(super) fn draw_dashboard(&mut self, ui: &mut egui::Ui) {
        self.catalog_bounds = Some(ui.available_rect_before_wrap());
        let mut scroll = egui::ScrollArea::vertical()
            .id_salt("cinematic-dashboard")
            .auto_shrink([false, false]);
        if self.reset_catalog_scroll {
            scroll = scroll.vertical_scroll_offset(0.0);
            self.reset_catalog_scroll = false;
        }
        scroll.show(ui, |ui| {
            let featured = self.featured_items();
            let feature = featured
                .get(self.feature_index.min(featured.len().saturating_sub(1)))
                .cloned();
            if let Some(item) = feature {
                let height = (ui.ctx().content_rect().height() * 0.49).clamp(330.0, 570.0);
                let (rect, _) = ui.allocate_exact_size(
                    Vec2::new(ui.available_width(), height),
                    egui::Sense::hover(),
                );
                self.hero_bounds = Some(rect);
                self.cinematic_image(ui, &item, rect, true, 0);
                Self::cinematic_fade(ui, rect);
                let left = rect.left() + 26.0;
                let base = rect.bottom() - 210.0;
                let width = (rect.width() * 0.48).min(650.0);
                if item.image_tags.contains_key("Logo")
                    && let Some(api) = &self.api
                {
                    let logo = egui::Rect::from_min_size(
                        egui::pos2(left, base - 28.0),
                        Vec2::new(width.min(340.0), 88.0),
                    );
                    egui::Image::new(api.logo(&item.id))
                        .fit_to_exact_size(logo.size())
                        .maintain_aspect_ratio(true)
                        .paint_at(ui, logo);
                } else {
                    design::text(
                        ui,
                        egui::Rect::from_min_size(egui::pos2(left, base), Vec2::new(width, 58.0)),
                        &item.name,
                        42.0,
                        design::TEXT,
                        1,
                    );
                }
                design::text(
                    ui,
                    egui::Rect::from_min_size(
                        egui::pos2(left, base + 64.0),
                        Vec2::new(width, 24.0),
                    ),
                    &metadata(&item),
                    14.0,
                    MUTED,
                    1,
                );
                design::text(
                    ui,
                    egui::Rect::from_min_size(
                        egui::pos2(left, base + 96.0),
                        Vec2::new(width, 44.0),
                    ),
                    &item.overview,
                    15.0,
                    design::TEXT,
                    2,
                );
                let cta = egui::Rect::from_min_size(
                    egui::pos2(left, rect.bottom() - 57.0),
                    Vec2::new(130.0, 42.0),
                );
                let r = ui.interact(cta, ui.id().with("featured-open"), egui::Sense::click());
                ui.painter().rect_filled(cta, 22.0, ACCENT);
                ui.painter().text(
                    cta.center(),
                    egui::Align2::CENTER_CENTER,
                    "▶  Explore",
                    egui::FontId::proportional(15.0),
                    BG,
                );
                if self.focus == self.items.len() + 6 {
                    ui.painter().rect_stroke(
                        cta.expand(4.0),
                        24.0,
                        Stroke::new(2.0_f32, design::FOCUS),
                        egui::StrokeKind::Outside,
                    );
                    self.focus_bounds = Some(cta);
                    if self.focus_changed {
                        ui.scroll_to_rect(rect, Some(egui::Align::Min));
                    }
                }
                for index in 0..featured.len() {
                    let dot = egui::Rect::from_center_size(
                        egui::pos2(
                            rect.center().x
                                + (index as f32 - (featured.len() - 1) as f32 / 2.0) * 18.0,
                            rect.bottom() - 18.0,
                        ),
                        Vec2::new(
                            if index == self.feature_index {
                                18.0
                            } else {
                                6.0
                            },
                            5.0,
                        ),
                    );
                    ui.painter().rect_filled(
                        dot,
                        3.0,
                        if index == self.feature_index {
                            design::TEXT
                        } else {
                            Color32::from_gray(95)
                        },
                    );
                    if ui
                        .interact(
                            dot.expand(6.0),
                            ui.id().with(("feature-dot", index)),
                            egui::Sense::click(),
                        )
                        .clicked()
                    {
                        self.feature_index = index;
                    }
                }
                if r.clicked() {
                    self.detail = Some(item);
                    self.detail_focus = 0;
                }
            }
            ui.add_space(12.0);
            self.shelf_offsets.clear();
            for shelf in self.shelves.clone() {
                Self::shelf_title(ui, &shelf.title);
                ui.add_space(12.0);
                let selected =
                    self.focus >= 6 + shelf.start && self.focus < 6 + shelf.start + shelf.count;
                let width = if shelf.landscape {
                    (ui.available_width() / 4.4).clamp(240.0, 390.0)
                } else {
                    (ui.available_width() / 7.4).clamp(150.0, 220.0)
                };
                let image_h = if shelf.landscape {
                    width * 9.0 / 16.0
                } else {
                    width * 1.5
                };
                let mut open = None;
                let shelf_scroll = egui::ScrollArea::horizontal()
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .id_salt(("shelf", &shelf.title))
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.add_space(4.0);
                        ui.horizontal_top(|ui| {
                            ui.add_space(6.0);
                            for index in shelf.start..shelf.start + shelf.count {
                                let item = &self.items[index];
                                let focused = self.focus == index + 6;
                                let (card, r) = ui.allocate_exact_size(
                                    Vec2::new(width, image_h + 56.0),
                                    egui::Sense::click(),
                                );
                                let art =
                                    egui::Rect::from_min_size(card.min, Vec2::new(width, image_h));
                                if ui.is_rect_visible(card) {
                                    self.cinematic_image(ui, item, art, false, 9);
                                    design::focus_ring(
                                        ui,
                                        art,
                                        ui.id().with(("shelf-glow", &shelf.title, index)),
                                        focused || r.hovered(),
                                        11.0,
                                    );
                                    if shelf.landscape {
                                        ui.painter().circle_filled(
                                            art.center(),
                                            21.0,
                                            Color32::from_black_alpha(145),
                                        );
                                        ui.painter().text(
                                            art.center(),
                                            egui::Align2::CENTER_CENTER,
                                            "▶",
                                            egui::FontId::proportional(18.0),
                                            design::TEXT,
                                        );
                                        let progress =
                                            item.user_data.played_percentage.unwrap_or(0.0) / 100.0;
                                        design::progress_bar(ui, art, progress);
                                    } else if let Some(count) =
                                        item.user_data.unplayed_item_count.filter(|n| *n > 0)
                                    {
                                        let badge =
                                            egui::pos2(art.right() - 18.0, art.top() + 18.0);
                                        ui.painter().circle_filled(badge, 14.0, ACCENT);
                                        ui.painter().text(
                                            badge,
                                            egui::Align2::CENTER_CENTER,
                                            count.to_string(),
                                            egui::FontId::proportional(12.0),
                                            BG,
                                        );
                                    }
                                    let title = if item.kind == "Episode" {
                                        item.series_name.as_deref().unwrap_or(&item.name)
                                    } else {
                                        &item.name
                                    };
                                    design::text(
                                        ui,
                                        egui::Rect::from_min_size(
                                            card.min + Vec2::new(0.0, image_h + 9.0),
                                            Vec2::new(width, 21.0),
                                        ),
                                        title,
                                        14.0,
                                        design::TEXT,
                                        1,
                                    );
                                    let sub = if item.kind == "Episode" {
                                        format!("{} · {}", item.episode_code(), item.name)
                                    } else {
                                        item.production_year
                                            .map(|y| y.to_string())
                                            .unwrap_or_default()
                                    };
                                    design::text(
                                        ui,
                                        egui::Rect::from_min_size(
                                            card.min + Vec2::new(0.0, image_h + 32.0),
                                            Vec2::new(width, 19.0),
                                        ),
                                        &sub,
                                        12.0,
                                        MUTED,
                                        1,
                                    );
                                }
                                if focused {
                                    self.focus_bounds = Some(card);
                                    if self.focus_changed {
                                        design::reveal_horizontal(ui, card);
                                    }
                                }
                                if r.clicked() {
                                    open = Some(index);
                                }
                                ui.add_space(12.0);
                            }
                        });
                    });
                self.shelf_offsets
                    .push((shelf.title.clone(), shelf_scroll.state.offset.x));
                if selected
                    && self.focus_changed
                    && let Some(rect) = self.focus_bounds
                {
                    design::reveal_vertical(ui, rect);
                }
                if let Some(index) = open {
                    self.activate_item(index);
                }
                ui.add_space(25.0);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vertical_wheel_and_row_changes_do_not_drag_other_shelves_sideways() {
        let ctx = egui::Context::default();
        let mut app = Aster::new(&ctx, Settings::default());
        app.items = (0..60)
            .map(|i| Item {
                id: i.to_string(),
                name: format!("Title {i}"),
                ..Default::default()
            })
            .collect();
        app.shelves = (0..3)
            .map(|row| Shelf {
                title: format!("Row {row}"),
                landscape: true,
                start: row * 20,
                count: 20,
            })
            .collect();
        app.focus = 6;
        let mut frame = 0;
        let render = |app: &mut Aster, frame: &mut u32, events: Vec<egui::Event>| {
            let _ = ctx.run(
                egui::RawInput {
                    time: Some(*frame as f64 / 60.0),
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(1440.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| app.draw_dashboard(ui));
                },
            );
            app.focus_changed = false;
            *frame += 1;
        };
        for _ in 0..40 {
            render(&mut app, &mut frame, vec![]);
        }
        let pointer = app.focus_bounds.unwrap().center();
        render(
            &mut app,
            &mut frame,
            vec![
                egui::Event::PointerMoved(pointer),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: Vec2::new(0.0, -90.0),
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        for _ in 0..40 {
            render(&mut app, &mut frame, vec![]);
        }
        assert!(
            app.shelf_offsets.iter().all(|(_, x)| x.abs() < 0.5),
            "vertical wheel was converted into horizontal scrolling: {:?}",
            app.shelf_offsets
        );
        app.focus = 16;
        app.focus_changed = true;
        app.reset_catalog_scroll = true;
        for _ in 0..60 {
            render(&mut app, &mut frame, vec![]);
        }
        let first = app.shelf_offsets[0].1;
        assert!(
            first > 100.0,
            "rightward focus must still reveal clipped cards"
        );
        app.dashboard_action(Action::Down);
        app.focus_changed = true;
        for _ in 0..60 {
            render(&mut app, &mut frame, vec![]);
        }
        assert!(
            (app.shelf_offsets[0].1 - first).abs() < 1.0,
            "entering another shelf moved the previous shelf"
        );
        let settled = app.shelf_offsets.clone();
        for _ in 0..4 {
            app.dashboard_action(Action::Up);
            app.focus_changed = true;
            for _ in 0..30 {
                render(&mut app, &mut frame, vec![]);
            }
            app.dashboard_action(Action::Down);
            app.focus_changed = true;
            for _ in 0..30 {
                render(&mut app, &mut frame, vec![]);
            }
        }
        for ((_, old), (_, new)) in settled.iter().zip(&app.shelf_offsets) {
            assert!(
                (old - new).abs() < 1.0,
                "vertical navigation caused a horizontal drift: {old} -> {new}"
            );
        }
    }
    #[test]
    fn controller_moves_between_uneven_shelves_and_featured_titles() {
        let ctx = egui::Context::default();
        let mut app = Aster::new(&ctx, Settings::default());
        app.items = (0..8)
            .map(|i| Item {
                id: i.to_string(),
                name: format!("Title {i}"),
                ..Item::default()
            })
            .collect();
        app.shelves = vec![
            Shelf {
                title: "Continue Watching".into(),
                landscape: true,
                start: 0,
                count: 5,
            },
            Shelf {
                title: "Latest in Movies".into(),
                landscape: false,
                start: 5,
                count: 3,
            },
        ];
        app.focus = 10;
        app.dashboard_action(Action::Right);
        assert_eq!(app.focus, 10, "must stay inside the shelf");
        app.dashboard_action(Action::Down);
        assert_eq!(app.focus, 13, "shorter rows clamp to the last card");
        app.dashboard_action(Action::Up);
        assert_eq!(app.focus, 8);
        app.dashboard_action(Action::Up);
        assert_eq!(app.focus, 14, "featured area must be controller reachable");
        app.dashboard_action(Action::Right);
        app.dashboard_action(Action::Confirm);
        assert_eq!(app.detail.as_ref().unwrap().id, "6");
        app.detail = None;
        app.dashboard_action(Action::Down);
        assert_eq!(app.focus, 6);
    }
    #[test]
    fn returning_to_discover_invalidates_shelf_ranges_before_new_items_arrive() {
        let ctx = egui::Context::default();
        let mut app = Aster::new(&ctx, Settings::default());
        app.items = vec![Item::default()];
        app.shelves = vec![Shelf {
            title: "Latest".into(),
            landscape: false,
            start: 0,
            count: 1,
        }];
        app.select_tab(1);
        assert!(app.items.is_empty());
        assert!(
            app.featured_items().is_empty(),
            "stale shelf ranges must never panic"
        );
        app.select_tab(0);
        assert!(
            app.shelves.is_empty(),
            "old shelf ranges must not render during a reload"
        );
    }
}
