use ksni::{menu::*, Icon, Tray, TrayMethods};
use tokio::process::Command;
use tokio::sync::mpsc::UnboundedSender;

fn read_exit_node() -> String {
    let path = std::env::var("HOME").expect("HOME not set") + "/.config/ts-switcher/exit_node.env";
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read {path}: {e}"))
        .trim()
        .to_string()
}

fn make_circle(filled: bool) -> Icon {
    const SIZE: i32 = 16;
    const CENTER: f32 = 7.5;
    const OUTER_R: f32 = 6.5;
    const INNER_R: f32 = 4.5;

    let mut data = Vec::with_capacity((SIZE * SIZE * 4) as usize);

    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 + 0.5 - CENTER;
            let dy = y as f32 + 0.5 - CENTER;
            let dist = (dx * dx + dy * dy).sqrt();

            let is_filled = if filled {
                dist <= OUTER_R
            } else {
                dist >= INNER_R && dist <= OUTER_R
            };

            if is_filled {
                data.extend_from_slice(&[0xFF, 0x88, 0x88, 0x88]);
            } else {
                data.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
            }
        }
    }

    Icon { width: SIZE, height: SIZE, data }
}

#[derive(Debug)]
struct AppTray {
    enabled: bool,
    tx: UnboundedSender<bool>,
}

impl Tray for AppTray {
    fn id(&self) -> String {
        "ts-switcher".into()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        vec![make_circle(self.enabled)]
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        vec![
            RadioGroup {
                selected: self.enabled as usize,
                select: Box::new(|this: &mut Self, index| {
                    let enable = index != 0;
                    this.enabled = enable; // optimistic update
                    let _ = this.tx.send(enable);
                }),
                options: vec![
                    RadioItem { label: "Disabled".into(), ..Default::default() },
                    RadioItem { label: "Enabled".into(), ..Default::default() },
                ],
                ..Default::default()
            }
            .into(),
        ]
    }
}

async fn tailscale_is_enabled() -> bool {
    Command::new("tailscale")
        .arg("status")
        .output()
        .await
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() != "Tailscale is stopped.")
        .unwrap_or(false)
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let exit_node = read_exit_node();
    let enabled = tailscale_is_enabled().await;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let handle: ksni::Handle<AppTray> = AppTray { enabled, tx }.spawn().await.unwrap();

    while let Some(enable) = rx.recv().await {
        let args: Vec<&str> = if enable {
            vec!["tailscale", "up", "--exit-node", &exit_node]
        } else {
            vec!["tailscale", "down"]
        };

        let success = Command::new("pkexec")
            .args(args)
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false);

        if !success {
            handle.update(|tray: &mut AppTray| tray.enabled = !enable).await;
        }
    }
}
