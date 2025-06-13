use crate::window_handle::{UserEvent, send_user_event};
use crate::{APP_NAME, APP_TITLE, ICON, ManagerMessages, ToMainMessages};
use anyhow::{Result, bail};
use beacn_lib::crossbeam;
use beacn_lib::crossbeam::channel::{Receiver, RecvError, Sender};
use beacn_lib::crossbeam::{channel, select};
use egui::Context;
use image::GenericImageView;
use ksni::blocking::{Handle, TrayMethods};
use ksni::{Category, Error, Icon, Status, ToolTip, Tray};
use log::{debug, error, warn};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::{env, fs};

enum TrayMessages {
    Activate,
}

pub fn handle_tray(
    tray_manager: Receiver<ManagerMessages>,
    tray_main_tx: Sender<ToMainMessages>,
) -> Result<()> {
    debug!("Spawning Tray");

    // Create a temporary directory to store the icon
    let tmp_file_dir = PathBuf::from(env::temp_dir().join(format!("{}", APP_NAME)));
    if !tmp_file_dir.exists() {
        fs::create_dir_all(&tmp_file_dir)?;
    }

    // Write the icon out to the temporary path
    let tmp_file_path = tmp_file_dir.join(format!("{}.png", APP_NAME));
    if !tmp_file_path.exists() || fs::remove_file(&tmp_file_path).is_ok() {
        fs::write(&tmp_file_path, ICON)?;
    } else {
        warn!("Unable to remove existing icon, using whatever is already there..");
    }

    let (icon_tx, icon_rx) = channel::bounded(20);
    let icon = TrayIcon::new(icon_tx, &tmp_file_path);
    let handle = icon.spawn_without_dbus_name()?;

    let mut egui_context = None;

    loop {
        select! {
            recv(icon_rx) -> msg => {
                match msg {
                    Ok(msg) => {
                        match msg {
                            TrayMessages::Activate => {
                                if let Some(context) = &egui_context {
                                    send_user_event(context, UserEvent::FocusWindow);
                                } else {
                                    // Tell the Main Thread to spawn a new window
                                    let _ = tray_main_tx.send(ToMainMessages::SpawnWindow);
                                }
                                debug!("Activate Triggered");
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Icon receiver channel broken, bailing: {}", e);
                        break;
                    }
                }
            }
            recv(tray_manager) -> msg => {
                match msg {
                    Ok(msg) => {
                        match msg {
                            ManagerMessages::SetContext(context) => {
                                egui_context = context;
                            }
                            ManagerMessages::Quit => {
                                break;
                            }
                        }
                    }

                    Err(e) => {
                        warn!("Message Handler channel Broken, bailing: {}", e);
                        break;
                    }
                }
            }
        }
    }

    debug!("Stopping Tray");
    if !handle.is_closed() {
        handle.shutdown();
    }

    // Remove the temporary icon file
    fs::remove_file(tmp_file_path)?;
    debug!("Tray Stopped");
    Ok(())
}

struct TrayIcon {
    icon: PathBuf,
    tx: Sender<TrayMessages>,
}

impl TrayIcon {
    fn new(tx: Sender<TrayMessages>, icon: &Path) -> Self {
        Self {
            icon: icon.to_path_buf(),
            tx,
        }
    }
}

impl Tray for TrayIcon {
    fn id(&self) -> String {
        APP_NAME.to_string()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.tx.send(TrayMessages::Activate);
    }
    fn category(&self) -> Category {
        Category::Hardware
    }
    fn title(&self) -> String {
        APP_TITLE.to_string()
    }
    fn status(&self) -> Status {
        Status::Active
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        static TRAY_ICON: LazyLock<Icon> = LazyLock::new(|| {
            let img = image::load_from_memory_with_format(ICON, image::ImageFormat::Png)
                .expect("Unable to Load Image");

            let (width, height) = img.dimensions();
            let mut data = img.into_rgba8().into_vec();

            for pixel in data.chunks_exact_mut(4) {
                pixel.rotate_right(1) // RGBA to ARGB
            }

            Icon {
                width: width as i32,
                height: height as i32,
                data,
            }
        });

        vec![TRAY_ICON.clone()]
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: String::from(APP_TITLE),
            description: String::from("A Tool for Configuring Beacn Devices"),
            ..Default::default()
        }
    }
}
