use winit::event_loop::EventLoopProxy;

#[derive(Debug, Clone)]
pub enum MenuAction {
    OpenRom,
    RomSelected(std::path::PathBuf),
    Reset,
    Stop,
    Quit,
    SaveState,
    LoadState,
    SetSlot(usize),
    ToggleFm,
    ShowControls,
    ShowAbout,
}

pub struct AppMenu;

impl AppMenu {
    pub fn build(proxy: EventLoopProxy<MenuAction>) -> Self {
        #[cfg(not(target_os = "linux"))]
        Self::build_native(proxy);
        #[cfg(target_os = "linux")]
        let _ = proxy; // egui menu bar used on Linux

        Self
    }

    #[cfg(not(target_os = "linux"))]
    fn build_native(proxy: EventLoopProxy<MenuAction>) {
        use muda::{Menu, Submenu, MenuItem, PredefinedMenuItem, MenuEvent};

        let menu = Menu::new();

        // Emulator submenu
        let open_rom   = MenuItem::new("Open ROM…", true, None);
        let reset      = MenuItem::new("Reset",     true, None);
        let stop       = MenuItem::new("Stop",      true, None);
        let quit       = MenuItem::new("Quit",      true, None);
        let emulator   = Submenu::with_items("Emulator", true, &[
            &open_rom,
            &PredefinedMenuItem::separator(),
            &reset,
            &stop,
            &PredefinedMenuItem::separator(),
            &quit,
        ]).unwrap();
        menu.append(&emulator).unwrap();

        // State submenu
        let save_state = MenuItem::new("Save State  [F7]", true, None);
        let load_state = MenuItem::new("Load State  [F5]", true, None);
        let mut slot_items: Vec<MenuItem> = (1..=9).map(|i| {
            MenuItem::new(format!("Slot {}", i), true, None)
        }).collect();
        let slot_submenu_items: Vec<&dyn muda::IsMenuItem> =
            slot_items.iter().map(|i| i as &dyn muda::IsMenuItem).collect();
        let slot_sub = Submenu::with_items("Slot", true, &slot_submenu_items).unwrap();
        let state_sub = Submenu::with_items("State", true, &[
            &save_state as &dyn muda::IsMenuItem,
            &load_state,
            &PredefinedMenuItem::separator(),
            &slot_sub,
        ]).unwrap();
        menu.append(&state_sub).unwrap();

        // Configuration submenu
        let controls = MenuItem::new("Controls…", true, None);
        let toggle_fm = MenuItem::new("Toggle FM Sound", true, None);
        let config_sub = Submenu::with_items("Configuration", true, &[
            &controls as &dyn muda::IsMenuItem,
            &PredefinedMenuItem::separator(),
            &toggle_fm,
        ]).unwrap();
        menu.append(&config_sub).unwrap();

        // About submenu
        let about_item = MenuItem::new("About vibe-sms…", true, None);
        let about_sub  = Submenu::with_items("About", true, &[
            &about_item as &dyn muda::IsMenuItem,
        ]).unwrap();
        menu.append(&about_sub).unwrap();

        // Capture IDs for dispatch
        let open_id    = open_rom.id().clone();
        let reset_id   = reset.id().clone();
        let stop_id    = stop.id().clone();
        let quit_id    = quit.id().clone();
        let save_id    = save_state.id().clone();
        let load_id    = load_state.id().clone();
        let slot_ids: Vec<_> = slot_items.iter().map(|i| i.id().clone()).collect();
        let fm_id      = toggle_fm.id().clone();
        let ctrl_id    = controls.id().clone();
        let about_id   = about_item.id().clone();

        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let action = if event.id == open_id {
                Some(MenuAction::OpenRom)
            } else if event.id == reset_id {
                Some(MenuAction::Reset)
            } else if event.id == stop_id {
                Some(MenuAction::Stop)
            } else if event.id == quit_id {
                Some(MenuAction::Quit)
            } else if event.id == save_id {
                Some(MenuAction::SaveState)
            } else if event.id == load_id {
                Some(MenuAction::LoadState)
            } else if event.id == fm_id {
                Some(MenuAction::ToggleFm)
            } else if event.id == ctrl_id {
                Some(MenuAction::ShowControls)
            } else if event.id == about_id {
                Some(MenuAction::ShowAbout)
            } else {
                slot_ids.iter().enumerate().find_map(|(i, id)| {
                    if event.id == *id { Some(MenuAction::SetSlot(i + 1)) } else { None }
                })
            };
            if let Some(a) = action {
                let _ = proxy.send_event(a);
            }
        }));

        // Store menu for window attachment
        // SAFETY: we store in a thread_local so attach_to_window can retrieve it
        NATIVE_MENU.with(|cell| {
            *cell.borrow_mut() = Some(menu);
        });
    }

    pub fn attach_to_window(&self, window: &winit::window::Window) {
        #[cfg(not(target_os = "linux"))]
        Self::do_attach(window);
        #[cfg(target_os = "linux")]
        let _ = window;
    }

    #[cfg(not(target_os = "linux"))]
    fn do_attach(window: &winit::window::Window) {
        use winit::raw_window_handle::HasWindowHandle;
        NATIVE_MENU.with(|cell| {
            if let Some(ref menu) = *cell.borrow() {
                #[cfg(target_os = "macos")]
                unsafe { menu.init_for_nsapp(); }

                #[cfg(target_os = "windows")]
                {
                    use winit::raw_window_handle::RawWindowHandle;
                    if let Ok(handle) = window.window_handle() {
                        if let RawWindowHandle::Win32(h) = handle.as_raw() {
                            unsafe { menu.init_for_hwnd(h.hwnd.get() as _).unwrap(); }
                        }
                    }
                }
            }
        });
    }
}

#[cfg(not(target_os = "linux"))]
use std::cell::RefCell;

#[cfg(not(target_os = "linux"))]
thread_local! {
    static NATIVE_MENU: RefCell<Option<muda::Menu>> = RefCell::new(None);
}
