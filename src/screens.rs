use super::*;

impl Launcher {
    pub(super) fn is_loading(&self) -> bool {
        self.phase == Phase::Launching
            || (self.library.games.is_empty() && self.status.starts_with("Reading"))
    }

    pub(super) fn keycap(&self, label: &str, s: f32) -> Div {
        let device = self.prompt("key", "xbox", "ps");
        if matches!(label, "Enter" | "Esc" | "F1" | "Q" | "E" | "M" | "V")
            || (label == "↑" && device == "key")
        {
            let label = label.to_string();
            let width = match label.as_str() {
                "Enter" => 48.,
                "Esc" => 36.,
                _ => 28.,
            };
            return div()
                .flex()
                .items_center()
                .justify_center()
                .flex_shrink_0()
                .w(px(width * s))
                .h(px(28. * s))
                .rounded(px(3. * s))
                .border_1()
                .border_color(rgba(0xffffff75))
                .bg(rgba(0x10151db0))
                .text_color(rgb(0xe1edf7))
                .font_weight(FontWeight::MEDIUM)
                .child(
                    canvas(
                        move |_, window, _| {
                            let run = window.text_style().to_run(label.len());
                            window
                                .text_system()
                                .shape_line(label.into(), px(13. * s), &[run], None)
                        },
                        move |bounds, line, window, cx| {
                            let _ = line.paint(
                                point(bounds.center().x - line.width / 2., bounds.top()),
                                px(26. * s),
                                window,
                                cx,
                            );
                        },
                    )
                    .w(px((width - 2.) * s))
                    .h(px(26. * s)),
                );
        }
        let glyph = match label {
            "Enter" => "key-enter".into(),
            "Esc" => "key-escape".into(),
            "F1" | "Q" | "E" | "M" | "V" => format!("key-{}", label.to_lowercase()),
            "A" | "B" | "X" | "Y" => format!("xbox-{}", label.to_lowercase()),
            "×" => "ps-cross".into(),
            "○" => "ps-circle".into(),
            "□" => "ps-square".into(),
            "△" => "ps-triangle".into(),
            "L1" => "ps-lb".into(),
            "R1" => "ps-rb".into(),
            "LB" => "xbox-lb".into(),
            "RB" => "xbox-rb".into(),
            "Start" => "xbox-menu".into(),
            "Options" => "ps-menu".into(),
            "↑" => format!("{device}-up"),
            "↑ ↓" => format!("{device}-scroll"),
            _ => format!("{device}-browse"),
        };
        let shoulder = matches!(label, "LB" | "RB" | "L1" | "R1");
        let face = matches!(label, "A" | "B" | "X" | "Y" | "×" | "○" | "□" | "△");
        div()
            .flex()
            .items_center()
            .justify_center()
            .flex_shrink_0()
            .w(px(if shoulder { 42. } else { 30. } * s))
            .h(px(30. * s))
            .rounded(px(15. * s))
            .when(face, |d| d.bg(rgba(0x10151db0)))
            .child(
                img(Arc::<Path>::from(
                    self.base
                        .join("assets/prompts")
                        .join(format!("{glyph}.png")),
                ))
                .w(px(if shoulder { 48. } else { 30. } * s))
                .h(px(if shoulder { 48. } else { 30. } * s))
                .flex_shrink_0()
                .object_fit(ObjectFit::Contain),
            )
    }

    pub(super) fn hint(&self, key: &str, label: &str, s: f32) -> Stateful<Div> {
        div()
            .id(SharedString::from(format!("hint-{label}")))
            .hover(|d| d.bg(rgba(0xffffff14)))
            .active(|d| d.bg(rgba(0xffffff25)))
            .flex()
            .h(px(36. * s))
            .px(px(14. * s))
            .rounded(px(3. * s))
            .gap(px(10. * s))
            .items_center()
            .text_size(px(14. * s))
            .text_color(rgb(0xf0f6fc))
            .child(self.keycap(key, s * 0.8))
            .child(label.to_string())
    }

    pub(super) fn details_page(
        &self,
        mut root: Stateful<Div>,
        game: &Game,
        w: f32,
        h: f32,
        s: f32,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let pad = 54. * s;
        let hero_h = h * 0.57;
        let hero = div()
            .absolute()
            .top_0()
            .w_full()
            .h(px(hero_h + 60. * s))
            .overflow_hidden()
            .child(self.backdrop(game, w, hero_h + 60. * s));
        root = root.child(hero);
        root = root
            .child(
                div()
                    .absolute()
                    .top_0()
                    .w_full()
                    .h(px(hero_h + 60. * s))
                    .bg(linear_gradient(
                        90.,
                        linear_color_stop(rgba(0x05080dc0), 0.),
                        linear_color_stop(rgba(0x05080d00), 0.7),
                    )),
            )
            .child(
                div()
                    .absolute()
                    .top_0()
                    .w_full()
                    .h(px(hero_h + 60. * s))
                    .bg(linear_gradient(
                        0.,
                        linear_color_stop(rgb(0x0b1015), 0.),
                        linear_color_stop(rgba(0x0b101500), 0.65),
                    )),
            )
            .child(
                div()
                    .id("details-back")
                    .absolute()
                    .left(px(pad))
                    .top(px(25. * s))
                    .cursor_pointer()
                    .child(self.hint(self.prompt("Esc", "B", "○"), "Back", s))
                    .on_click(
                        cx.listener(|this, _, window, cx| this.input(Input::Back, window, cx)),
                    ),
            );
        let mut title = div()
            .absolute()
            .left(px(pad))
            .top(px(hero_h - 340. * s))
            .w(px(450. * s))
            .h(px(220. * s));
        title = if game.logo.is_empty() {
            title
                .pt(px(75. * s))
                .text_size(px(42. * s))
                .font_weight(FontWeight::BOLD)
                .child(game.name.clone())
        } else {
            title.child(
                img(Self::image(&game.logo))
                    .absolute()
                    .size_full()
                    .object_fit(ObjectFit::Contain),
            )
        };
        root = root.child(title).child(
            div()
                .absolute()
                .left(px(pad))
                .top(px(hero_h - 90. * s))
                .flex()
                .gap(px(20. * s))
                .items_center()
                .child(
                    self.button("details-play", "Play".into(), self.details_action == 0, s)
                        .w(px(160. * s))
                        .when(self.details_action == 0, |d| {
                            d.child(self.keycap(self.prompt("Enter", "A", "\u{00d7}"), s * 0.7))
                        })
                        .on_click(cx.listener(|this, _, window, cx| this.launch(window, cx))),
                )
                .when(!game.video.is_empty(), |d| {
                    d.child(
                        self.button(
                            "details-video",
                            "Watch video".into(),
                            self.details_action == 1,
                            s,
                        )
                        .w(px(200. * s))
                        .child(self.keycap(
                            if self.details_action == 1 {
                                self.prompt("Enter", "A", "\u{00d7}")
                            } else {
                                self.prompt("V", "Y", "\u{25b3}")
                            },
                            s * 0.7,
                        ))
                        .on_click(cx.listener(|this, _, window, cx| this.watch_video(window, cx))),
                    )
                })
                .child(
                    div()
                        .text_size(px(14. * s))
                        .text_color(rgb(0xb3c0cc))
                        .child(
                            [game.year.clone(), game.genres.clone()]
                                .into_iter()
                                .filter(|x| !x.is_empty())
                                .collect::<Vec<_>>()
                                .join("   ·   "),
                        ),
                ),
        );
        let content_h = h - hero_h - 85. * s;
        let mut metadata = div()
            .w(px(205. * s))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap(px(18. * s));
        for (label, value) in [
            ("Platform", game.platform.clone()),
            ("Developer", game.developer.clone()),
            (
                "Time played",
                if game.playtime == 0 {
                    "Not played yet".into()
                } else {
                    format!("{}h {}m", game.playtime / 3600, game.playtime / 60 % 60)
                },
            ),
        ] {
            if !value.is_empty() {
                metadata = metadata.child(
                    div()
                        .child(
                            div()
                                .text_size(px(11. * s))
                                .text_color(rgb(0x8294a5))
                                .child(label),
                        )
                        .child(
                            div()
                                .mt(px(5. * s))
                                .text_size(px(15. * s))
                                .text_color(rgb(0xd4e1ed))
                                .child(value),
                        ),
                );
            }
        }
        let mut body = div()
            .absolute()
            .left(px(pad))
            .top(px(hero_h + 10. * s))
            .w(px(w - pad * 2.))
            .h(px(content_h))
            .flex()
            .gap(px(32. * s));
        if !game.cover.is_empty() {
            body = body.child(
                img(Self::image(&game.cover))
                    .w(px(142. * s))
                    .h(px(content_h.min(210. * s)))
                    .flex_shrink_0()
                    .rounded(px(8. * s))
                    .object_fit(ObjectFit::Cover),
            );
        }
        root.child(
            body.child(metadata).child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(
                        div()
                            .text_size(px(19. * s))
                            .font_weight(FontWeight::SEMIBOLD)
                            .mb(px(14. * s))
                            .child("About the game"),
                    )
                    .child(
                        div()
                            .id("description")
                            .track_scroll(&self.details_scroll)
                            .overflow_y_scroll()
                            .h(px(content_h - 42. * s))
                            .text_size(px(16. * s))
                            .line_height(relative(1.65))
                            .text_color(rgb(0xbac7d3))
                            .child(if game.description.is_empty() {
                                "No description available.".into()
                            } else {
                                game.description.clone()
                            }),
                    ),
            ),
        )
    }

    pub(super) fn video_page(
        &self,
        mut root: Stateful<Div>,
        s: f32,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        root = root.bg(rgb(0x000000));
        if let Some(frame) = &self.video_frame {
            root = root.child(
                img(frame.clone())
                    .absolute()
                    .size_full()
                    .object_fit(ObjectFit::Contain),
            );
        } else {
            root = root.child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(18. * s))
                    .child("Loading video"),
            );
        }
        root.child(
            div().absolute().left(px(54. * s)).top(px(25. * s)).child(
                self.hint(self.prompt("Esc", "B", "\u{25cb}"), "Back", s)
                    .bg(rgba(0x000000aa))
                    .cursor_pointer()
                    .on_click(
                        cx.listener(|this, _, window, cx| this.input(Input::Back, window, cx)),
                    ),
            ),
        )
    }

    pub(super) fn status_page(
        &self,
        mut root: Stateful<Div>,
        game: &Game,
        s: f32,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let loading = self.is_loading();
        let elapsed = if loading {
            self.animation_frame as f32 * 0.033
        } else {
            0.
        };
        // Native GPUI paths are tessellated and rendered by the GPU. No bitmap animation or video decoder.
        if loading {
            root = root.child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        let center = bounds.center();
                        let cx: f32 = center.x.into();
                        let cy: f32 = center.y.into();
                        for ring in 0..9 {
                            let r = ring as f32;
                            let mut path =
                                PathBuilder::stroke(px((if ring == 0 { 1.6 } else { 0.8 }) * s));
                            for step in 0..=128 {
                                let a = step as f32 / 128. * std::f32::consts::TAU;
                                let twist = elapsed * 0.23 + r * 0.13;
                                let radius = 1. + 0.04 * (a * 3. + elapsed * 0.6 + r * 0.4).sin();
                                let x = a.cos() * (340. + r * 9.) * radius;
                                let y = a.sin() * (150. + r * 7.) * radius;
                                let p = point(
                                    px(cx + (x * twist.cos() - y * twist.sin()) * s),
                                    px(cy - 24. * s + (x * twist.sin() + y * twist.cos()) * s),
                                );
                                if step == 0 {
                                    path.move_to(p);
                                } else {
                                    path.line_to(p);
                                }
                            }
                            if let Ok(path) = path.build() {
                                window.paint_path(
                                    path,
                                    rgba(if ring == 0 { 0xa8d8ff88 } else { 0x7cbbef22 }),
                                );
                            }
                        }
                        for i in 0..7 {
                            let a = elapsed * (0.23 + i as f32 * 0.009) + i as f32 * 0.897;
                            let x = cx + a.cos() * (365. + i as f32 * 5.) * s;
                            let y = cy - 24. * s + a.sin() * (205. + i as f32 * 5.) * s;
                            window.paint_quad(quad(
                                Bounds::new(point(px(x), px(y)), size(px(3. * s), px(3. * s))),
                                px(2. * s),
                                rgba(0xc8e6ffb0),
                                px(0.),
                                transparent_black(),
                                Default::default(),
                            ));
                        }
                    },
                )
                .absolute()
                .size_full(),
            );
        }
        let (title, subtitle) = match &self.phase {
            Phase::Launching => ("Starting", String::new()),
            Phase::Running => ("Game running", String::new()),
            Phase::Failed(e) => ("Couldn't start this game", e.clone()),
            Phase::Library if loading => ("Loading library", String::new()),
            _ => ("No games", String::new()),
        };
        root = root.child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(25. * s))
                .when(self.phase != Phase::Running && !game.logo.is_empty(), |d| {
                    d.child(
                        div().relative().w(px(380. * s)).h(px(135. * s)).child(
                            img(Self::image(&game.logo))
                                .absolute()
                                .size_full()
                                .object_fit(ObjectFit::Contain),
                        ),
                    )
                })
                .when(game.logo.is_empty() && !game.name.is_empty(), |d| {
                    d.child(div().text_size(px(32. * s)).child(game.name.clone()))
                })
                .child(
                    div()
                        .w(px(650. * s))
                        .h(px(42. * s))
                        .flex_shrink_0()
                        .text_center()
                        .text_size(px(if loading { 18. } else { 26. } * s))
                        .font_weight(FontWeight::MEDIUM)
                        .child(
                            canvas(
                                move |_, window, _| {
                                    let run = window.text_style().to_run(title.len());
                                    window.text_system().shape_line(
                                        title.into(),
                                        px(if loading { 18. } else { 26. } * s),
                                        &[run],
                                        None,
                                    )
                                },
                                move |bounds, line, window, cx| {
                                    let origin =
                                        point(bounds.center().x - line.width / 2., bounds.top());
                                    let _ = line.paint(origin, px(42. * s), window, cx);
                                },
                            )
                            .w(px(650. * s))
                            .h(px(42. * s)),
                        ),
                )
                .when(!subtitle.is_empty(), |d| {
                    d.child(
                        div()
                            .w(px(620. * s))
                            .text_center()
                            .text_size(px(16. * s))
                            .text_color(rgb(0xa8b9ca))
                            .child(subtitle),
                    )
                }),
        );
        root.when(
            self.phase != Phase::Library,
            |root| {
                root.child(
                    div()
                        .absolute()
                        .left(px(54. * s))
                        .bottom(px(25. * s))
                        .h(px(36. * s))
                        .child(
                            div()
                                .id("status-back")
                                .cursor_pointer()
                                .child(self.hint(self.prompt("Esc", "B", "○"), "Back", s))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.input(Input::Back, window, cx)
                                })),
                        ),
                )
            },
        )
    }

    pub(super) fn menu_layer(
        &self,
        mut root: Stateful<Div>,
        s: f32,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let progress = self.menu_motion.value(Instant::now());
        if progress <= 0. || self.details || self.watching_video || self.phase != Phase::Library {
            return root;
        }
        let labels = [
            ("Back to games", None),
            ("Music", Some(self.config.music)),
            ("Background video", Some(self.config.video)),
            ("Close launcher", None),
        ];
        let mut rows = div().flex().flex_col().gap(px(8. * s));
        for (i, (label, enabled)) in labels.into_iter().enumerate() {
            let selected = i == self.menu_index;
            let row = div()
                .id(("menu-item", i))
                .relative()
                .cursor_pointer()
                .h(px(62. * s))
                .px(px(20. * s))
                .rounded(px(10. * s))
                .flex()
                .items_center()
                .justify_between()
                .bg(if selected { rgb(0x253848) } else { rgba(0) })
                .hover(|d| d.bg(rgb(0x2c4051)))
                .active(|d| d.bg(rgb(0x3e566b)))
                .text_color(if selected {
                    rgb(0xf4f8fc)
                } else {
                    rgb(0xc4d0da)
                })
                .text_size(px(17. * s))
                .font_weight(FontWeight::MEDIUM)
                .when(selected, |d| {
                    d.child(
                        div()
                            .absolute()
                            .left_0()
                            .top(px(20. * s))
                            .w(px(3. * s))
                            .h(px(22. * s))
                            .rounded(px(2. * s))
                            .bg(rgb(0xa8d8ff)),
                    )
                })
                .child(label)
                .when_some(enabled, |d, enabled| {
                    d.child(
                        div()
                            .w(px(36. * s))
                            .h(px(20. * s))
                            .rounded(px(10. * s))
                            .bg(if enabled {
                                rgb(0xa8d8ff)
                            } else {
                                rgb(0x536272)
                            })
                            .child(
                                div()
                                    .mt(px(3. * s))
                                    .ml(px(if enabled { 19. } else { 3. } * s))
                                    .size(px(14. * s))
                                    .rounded(px(7. * s))
                                    .bg(if enabled {
                                        rgb(0x122538)
                                    } else {
                                        rgb(0xd0d9e1)
                                    }),
                            ),
                    )
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    if this.menu {
                        this.menu_index = i;
                        this.input(Input::Accept, window, cx);
                    }
                }));
            if i == 3 {
                rows = rows.child(
                    div()
                        .h(px(1.))
                        .mx(px(20. * s))
                        .my(px(14. * s))
                        .bg(rgba(0xffffff15)),
                );
            }
            rows = rows.child(row);
        }
        let panel = div()
            .occlude()
            .absolute()
            .left_0()
            .top_0()
            .bottom_0()
            .w(px(410. * s))
            .px(px(34. * s))
            .bg(rgba(0x0b121bf5))
            .border_r_1()
            .border_color(rgba(0xffffff14))
            .flex()
            .flex_col()
            .justify_center()
            .child(
                div()
                    .text_size(px(30. * s))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0xf4f8fc))
                    .mb(px(30. * s))
                    .px(px(20. * s))
                    .child("Menu"),
            )
            .child(rows)
            .child(
                div()
                    .absolute()
                    .bottom(px(32. * s))
                    .left(px(34. * s))
                    .right(px(34. * s))
                    .flex()
                    .justify_between()
                    .child(
                        self.hint(self.prompt("Enter", "A", "\u{00d7}"), "Select", s)
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, window, cx| {
                                if this.menu {
                                    this.input(Input::Accept, window, cx);
                                }
                            })),
                    )
                    .child(
                        self.hint(self.prompt("Esc", "B", "\u{25cb}"), "Back", s)
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.input(Input::Back, window, cx)
                            })),
                    ),
            );
        root = root.child(
            div()
                .absolute()
                .inset_0()
                .occlude()
                .bg(rgba(0x03070b00 | (136. * progress) as u32)),
        );
        root.child(panel.left(px((1. - progress) * -36. * s)).opacity(progress))
    }
}
