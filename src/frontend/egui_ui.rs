use std::sync::Arc;
use egui::Context;
use egui_winit::State as WinitState;
use egui_glow::Painter;
use winit::window::Window;
use winit::event::WindowEvent;

use crate::frontend::input::{KeyConfig, key_label};
use crate::frontend::menu::MenuAction;

pub struct EguiState {
    pub ctx:         Context,
    winit_state:     WinitState,
    painter:         Painter,
}

impl EguiState {
    pub fn new(gl: Arc<glow::Context>, window: &Window) -> Self {
        let ctx = Context::default();
        let viewport_id = egui::ViewportId::default();
        let winit_state = WinitState::new(ctx.clone(), viewport_id, window, None, None, None);
        let painter = Painter::new(gl, "", None, false).unwrap();
        Self { ctx, winit_state, painter }
    }

    /// Free all GPU resources.  Must be called while the GL context is still current.
    pub fn destroy(&mut self) {
        self.painter.destroy();
    }

    /// Returns true if the event was consumed by egui.
    pub fn handle_window_event(&mut self, window: &Window, event: &WindowEvent) -> bool {
        let response = self.winit_state.on_window_event(window, event);
        response.consumed
    }

    pub fn run_frame(
        &mut self,
        window: &Window,
        _gl: &Arc<glow::Context>,
        dialog: &mut DialogState,
        menu_tx: &dyn Fn(MenuAction),
        is_gg: bool,
        is_sg: bool,
    ) {
        let raw_input = self.winit_state.take_egui_input(window);
        let full_output = self.ctx.run(raw_input, |ctx| {
            draw_dialogs(ctx, dialog, menu_tx, is_gg, is_sg);
        });
        self.winit_state.handle_platform_output(window, full_output.platform_output.clone());
        let clipped = self.ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
        let dims = window.inner_size();
        self.painter.paint_and_update_textures(
            [dims.width, dims.height],
            full_output.pixels_per_point,
            &clipped,
            &full_output.textures_delta,
        );
    }
}

pub struct DialogState {
    pub show_key_config:  bool,
    pub show_about:       bool,
    pub show_fm_notice:   bool,
    pub show_slot_hud:    u8,
    pub save_slot:        usize,
    pub binding:          Option<(usize, usize)>,
    pub key_config:       KeyConfig,
    pub fm_disabled:      bool,
    pub rom_loaded:       bool,
    /// Height of the egui menu bar in egui points (Linux only; 0 elsewhere).
    pub menu_bar_height:  f32,
}

fn draw_dialogs(
    ctx: &egui::Context,
    d: &mut DialogState,
    menu_tx: &dyn Fn(MenuAction),
    is_gg: bool,
    is_sg: bool,
) {
    // ── Linux-only egui menu bar ─────────────────────────────────────────────────
    #[cfg(target_os = "linux")]
    draw_linux_menu(ctx, d, menu_tx, is_gg, is_sg);
    #[cfg(not(target_os = "linux"))]
    { d.menu_bar_height = 0.0; }

    // ── Slot HUD overlay ─────────────────────────────────────────────────────────
    if d.show_slot_hud > 0 {
        d.show_slot_hud -= 1;
        let alpha = ((d.show_slot_hud as f32 / 90.0) * 220.0) as u8;
        let hud = format!("Slot {}", d.save_slot);
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("slot_hud"),
        ));
        let pos = egui::pos2(16.0, 40.0);
        painter.text(
            pos + egui::vec2(1.0, 1.0),
            egui::Align2::LEFT_TOP,
            &hud,
            egui::FontId::proportional(20.0),
            egui::Color32::from_black_alpha(alpha),
        );
        painter.text(
            pos,
            egui::Align2::LEFT_TOP,
            &hud,
            egui::FontId::proportional(20.0),
            egui::Color32::from_rgba_unmultiplied(255, 255, 0, alpha),
        );
    }

    // ── Controls window ──────────────────────────────────────────────────────────
    let mut show_key_config = d.show_key_config;
    egui::Window::new("Controls")
        .open(&mut show_key_config)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            const ACTIONS: &[&str] = &["Up", "Down", "Left", "Right", "Button 1", "Button 2", "Start/Pause"];
            egui::Grid::new("ctrl_grid").num_columns(3).striped(true).show(ui, |ui| {
                ui.strong("Action"); ui.strong("Player 1"); ui.strong("Player 2");
                ui.end_row();
                for (ai, &name) in ACTIONS.iter().enumerate() {
                    ui.label(name);
                    for pi in 0..2usize {
                        let keys = if pi == 0 { &d.key_config.p1 } else { &d.key_config.p2 };
                        let waiting = d.binding == Some((pi, ai));
                        let lbl = if waiting { "Press any key…".to_string() }
                                  else       { key_label(keys.get(ai)).to_string() };
                        if ui.button(lbl).clicked() {
                            d.binding = if waiting { None } else { Some((pi, ai)) };
                        }
                    }
                    ui.end_row();
                }
            });
            ui.separator();
            if ui.button("Reset to defaults").clicked() {
                d.key_config = KeyConfig::default();
            }
        });
    d.show_key_config = show_key_config;

    // ── FM notice ────────────────────────────────────────────────────────────────
    if d.show_fm_notice {
        egui::Window::new("FM Sound Changed")
            .collapsible(false).resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label("FM sound setting changed.");
                ui.label("Reset the game (Emulator → Reset) for the change to take effect.");
                ui.separator();
                if ui.button("  OK  ").clicked() { d.show_fm_notice = false; }
            });
    }

    // ── About window ─────────────────────────────────────────────────────────────
    let mut show_about = d.show_about;
    egui::Window::new("About vibe-sms")
        .open(&mut show_about)
        .collapsible(false).resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.heading("vibe-sms");
            ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
            ui.separator();
            ui.label("Sega Master System / Game Gear emulator — written in Rust.");
            ui.label("Supports FM sound (YM2413), PSG (SN76489), gamepad, and Light Phaser.");
            ui.separator();
            ui.label("Built with:  egui · winit · cpal · gilrs · z80");
            ui.separator();
            ui.label("Created using Google Antigravity powered by Gemini.");
        });
    d.show_about = show_about;
}

#[cfg(target_os = "linux")]
fn draw_linux_menu(
    ctx: &egui::Context,
    d: &mut DialogState,
    menu_tx: &dyn Fn(MenuAction),
    is_gg: bool,
    is_sg: bool,
) {
    let frame = egui::Frame::new()
        .fill(egui::Color32::TRANSPARENT)
        .inner_margin(egui::Margin::symmetric(4i8, 2i8));
    let panel_resp = egui::TopBottomPanel::top("menu_bar").frame(frame).show(ctx, |ui| {
        egui::MenuBar::new().ui(ui, |ui| {
            // Emulator
            ui.menu_button("Emulator", |ui| {
                if ui.button("Open ROM…").clicked() { ui.close(); menu_tx(MenuAction::OpenRom); }
                ui.separator();
                ui.add_enabled_ui(d.rom_loaded, |ui| {
                    if ui.button("Reset").clicked() { ui.close(); menu_tx(MenuAction::Reset); }
                    if ui.button("Stop").clicked()  { ui.close(); menu_tx(MenuAction::Stop);  }
                });
                ui.separator();
                if ui.button("Quit").clicked() { ui.close(); menu_tx(MenuAction::Quit); }
            });
            // State
            ui.menu_button("State", |ui| {
                ui.add_enabled_ui(d.rom_loaded, |ui| {
                    if ui.button("Save State  [F7]").clicked() {
                        ui.close(); menu_tx(MenuAction::SaveState);
                    }
                    if ui.button("Load State  [F5]").clicked() {
                        ui.close(); menu_tx(MenuAction::LoadState);
                    }
                    ui.separator();
                    ui.menu_button(format!("Slot  [{}]", d.save_slot), |ui| {
                        for slot in 1..=9usize {
                            let label = format!("{} Slot {}", if slot == d.save_slot { "✓" } else { "  " }, slot);
                            if ui.button(label).clicked() {
                                ui.close(); menu_tx(MenuAction::SetSlot(slot));
                            }
                        }
                    });
                });
            });
            // Configuration
            ui.menu_button("Configuration", |ui| {
                if ui.button("Controls…").clicked() {
                    ui.close(); menu_tx(MenuAction::ShowControls);
                }
                ui.separator();
                let mut fm_on = !d.fm_disabled;
                let changed = ui.add_enabled(!is_gg && !is_sg, egui::Checkbox::new(&mut fm_on, "FM Sound")).changed();
                if changed { menu_tx(MenuAction::ToggleFm); }
                if is_gg || is_sg {
                    ui.label(egui::RichText::new("(SMS only)").small().color(egui::Color32::GRAY));
                }
            });
            // About
            ui.menu_button("About", |ui| {
                if ui.button("About vibe-sms…").clicked() {
                    ui.close(); menu_tx(MenuAction::ShowAbout);
                }
            });
        });
    });
    d.menu_bar_height = panel_resp.response.rect.height();
}
