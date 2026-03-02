use ksni::{menu::*, Icon, Tray, TrayMethods};
use notify_rust::Notification;
use reqwest::Client;
use tokio::process::Command;
use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug, Clone)]
struct ExitNode {
    ip: String,
    hostname: String,
    is_active: bool,    // currently in use (STATUS starts with "selected" and tailscale is running)
    is_available: bool, // online and can be selected (STATUS does not start with "offline")
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

fn parse_exit_nodes(output: &str) -> Vec<ExitNode> {
    output
        .lines()
        .filter(|line| {
            let t = line.trim();
            !t.is_empty() && !t.starts_with('#') && !t.starts_with("IP")
        })
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let ip = parts.next()?.to_string();
            let hostname = parts.next()?.to_string();
            let _country = parts.next()?;
            let _city = parts.next()?;
            let status = parts.collect::<Vec<_>>().join(" ");
            Some(ExitNode {
                ip,
                hostname,
                is_active: status.starts_with("selected"),
                is_available: !status.starts_with("offline"),
            })
        })
        .collect()
}

async fn notify(summary: &str, body: &str, icon: &str) {
    let _ = Notification::new()
        .appname("ts-switcher")
        .summary(summary)
        .body(body)
        .icon(icon)
        .show_async()
        .await;
}

async fn tailscale_is_running() -> bool {
    Command::new("tailscale")
        .arg("status")
        .output()
        .await
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() != "Tailscale is stopped.")
        .unwrap_or(false)
}

async fn fetch_exit_nodes() -> Vec<ExitNode> {
    let (list_output, running) =
        tokio::join!(Command::new("tailscale").args(["exit-node", "list"]).output(), tailscale_is_running());

    match list_output {
        Ok(output) => {
            let mut nodes = parse_exit_nodes(&String::from_utf8_lossy(&output.stdout));
            if !running {
                for node in &mut nodes {
                    node.is_active = false;
                }
            }
            nodes
        }
        Err(e) => {
            eprintln!("[fetch_exit_nodes] failed: {e}");
            vec![]
        }
    }
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

#[derive(Debug)]
struct AppTray {
    exit_nodes: Vec<ExitNode>,
    location: String,
    tx: UnboundedSender<Option<String>>,
}

impl AppTray {
    fn is_enabled(&self) -> bool {
        self.exit_nodes.iter().any(|n| n.is_active)
    }
}

impl Tray for AppTray {
    fn id(&self) -> String {
        "ts-switcher".into()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        vec![make_circle(self.is_enabled())]
    }


    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        let enabled = self.is_enabled();
        let (selectable, unselectable): (Vec<&ExitNode>, Vec<&ExitNode>) =
            self.exit_nodes.iter().partition(|n| n.is_available);

        let mut items: Vec<ksni::MenuItem<Self>> = vec![
            StandardItem {
                label: self.location.clone(),
                enabled: false,
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            CheckmarkItem {
                label: "Disabled".into(),
                checked: !enabled,
                activate: Box::new(|this: &mut Self| {
                    if this.is_enabled() {
                        for node in &mut this.exit_nodes {
                            node.is_active = false;
                        }
                        this.location = "Fetching...".into();
                        let _ = this.tx.send(None);
                    }
                }),
                ..Default::default()
            }
            .into(),
        ];

        for node in selectable {
            let ip = node.ip.clone();
            items.push(
                CheckmarkItem {
                    label: node.hostname.clone(),
                    checked: node.is_active,
                    activate: Box::new(move |this: &mut Self| {
                        for n in &mut this.exit_nodes {
                            n.is_active = n.ip == ip;
                        }
                        this.location = "Fetching...".into();
                        let _ = this.tx.send(Some(ip.clone()));
                    }),
                    ..Default::default()
                }
                .into(),
            );
        }

        if !unselectable.is_empty() {
            items.push(MenuItem::Separator);
            for node in unselectable {
                items.push(
                    StandardItem {
                        label: node.hostname.clone(),
                        enabled: false,
                        ..Default::default()
                    }
                    .into(),
                );
            }
        }

        items
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let client = Client::new();

    let (exit_nodes, location) =
        tokio::join!(fetch_exit_nodes(), fetch_location(&client));

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Option<String>>();
    let handle: ksni::Handle<AppTray> = AppTray {
        exit_nodes: exit_nodes.clone(),
        location: location.clone(),
        tx,
    }
    .spawn()
    .await
    .unwrap();

    let mut confirmed_location = location;
    let mut exit_nodes = exit_nodes.clone();
    let mut refresh = tokio::time::interval(std::time::Duration::from_secs(60));
    refresh.tick().await; // skip the immediate first tick

    loop {
        let action = tokio::select! {
            Some(a) = rx.recv() => a,
            _ = refresh.tick() => {
                let (new_nodes, new_location) =
                    tokio::join!(fetch_exit_nodes(), fetch_location(&client));
                exit_nodes = new_nodes.clone();
                confirmed_location = new_location.clone();
                let _ = handle.update(|tray: &mut AppTray| {
                    tray.exit_nodes = new_nodes;
                    tray.location = new_location;
                }).await;
                continue;
            }
        };
        let args: Vec<&str> = match &action {
            None => vec!["tailscale", "down"],
            Some(ip) => vec!["tailscale", "up", "--exit-node", ip.as_str()],
        };

        let success = Command::new(args[0])
            .args(&args[1..])
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false);

        if success {
            match &action {
                None => notify("Tailscale", "Exit node disconnected", "network-offline").await,
                Some(ip) => {
                    let hostname = exit_nodes
                        .iter()
                        .find(|n| &n.ip == ip)
                        .map(|n| n.hostname.as_str())
                        .unwrap_or(ip.as_str());
                    notify("Tailscale", &format!("Connected via {hostname}"), "network-vpn").await;
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            let (new_nodes, new_location) =
                tokio::join!(fetch_exit_nodes(), fetch_location(&client));
            exit_nodes = new_nodes.clone();
            confirmed_location = new_location.clone();
            let _ = handle
                .update(|tray: &mut AppTray| {
                    tray.exit_nodes = new_nodes;
                    tray.location = new_location;
                })
                .await;
        } else {
            match &action {
                None => notify("Tailscale", "Failed to disconnect", "dialog-error").await,
                Some(ip) => {
                    let hostname = exit_nodes
                        .iter()
                        .find(|n| &n.ip == ip)
                        .map(|n| n.hostname.as_str())
                        .unwrap_or(ip.as_str());
                    notify("Tailscale", &format!("Failed to connect via {hostname}"), "dialog-error").await;
                }
            }
            let old_nodes = fetch_exit_nodes().await;
            let rollback_location = confirmed_location.clone();
            let _ = handle
                .update(|tray: &mut AppTray| {
                    tray.exit_nodes = old_nodes;
                    tray.location = rollback_location;
                })
                .await;
        }
    }
}
