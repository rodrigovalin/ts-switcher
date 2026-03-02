use ksni::{menu::*, Icon, Tray, TrayMethods};
use reqwest::Client;
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

async fn fetch_location(client: &Client) -> String {
    for attempt in 0..3 {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
        eprintln!("[fetch_location] attempt {}/3...", attempt + 1);
        let result = async {
            let resp = client
                .get("https://ipinfo.io/json")
                .timeout(std::time::Duration::from_secs(2))
                .send()
                .await?
                .json::<serde_json::Value>()
                .await?;

            let ip = resp["ip"].as_str().unwrap_or("?");
            let city = resp["city"].as_str().unwrap_or("?");
            let country = resp["country"].as_str().unwrap_or("?");

            Ok::<String, reqwest::Error>(format!("{ip} ({city}, {country})"))
        }
        .await;

        match result {
            Ok(location) => {
                eprintln!("[fetch_location] got: {location}");
                return location;
            }
            Err(e) => eprintln!("[fetch_location] failed: {e}"),
        }
    }
    eprintln!("[fetch_location] all attempts failed, giving up");
    "Unknown location".into()
}

async fn tailscale_is_enabled() -> bool {
    Command::new("tailscale")
        .arg("status")
        .output()
        .await
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() != "Tailscale is stopped.")
        .unwrap_or(false)
}

#[derive(Debug)]
struct AppTray {
    enabled: bool,
    location: String,
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
            StandardItem {
                label: self.location.clone(),
                enabled: false,
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            CheckmarkItem {
                label: "Disabled".into(),
                checked: !self.enabled,
                activate: Box::new(|this: &mut Self| {
                    if this.enabled {
                        this.enabled = false;
                        this.location = "Fetching...".into();
                        let _ = this.tx.send(false);
                    }
                }),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: "Enabled".into(),
                checked: self.enabled,
                activate: Box::new(|this: &mut Self| {
                    if !this.enabled {
                        this.enabled = true;
                        this.location = "Fetching...".into();
                        let _ = this.tx.send(true);
                    }
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let exit_node = read_exit_node();
    let client = Client::new();

    let (enabled, location) =
        tokio::join!(tailscale_is_enabled(), fetch_location(&client));

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let handle: ksni::Handle<AppTray> =
        AppTray { enabled, location: location.clone(), tx }.spawn().await.unwrap();

    let mut confirmed_location = location;

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

        if success {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            let new_location = fetch_location(&client).await;
            confirmed_location = new_location.clone();
            let _ = handle.update(|tray: &mut AppTray| tray.location = new_location).await;
        } else {
            let rollback_location = confirmed_location.clone();
            let _ = handle
                .update(|tray: &mut AppTray| {
                    tray.enabled = !enable;
                    tray.location = rollback_location;
                })
                .await;
        }
    }
}
