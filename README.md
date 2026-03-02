# ts-switcher

A GNOME system tray applet to manage Tailscale exit nodes.

## What it does

- Shows a grey circle icon in the GNOME top bar
  - **Hollow circle** — no exit node active (Tailscale is stopped)
  - **Filled circle** — an exit node is active
- Clicking the icon opens a menu showing:
  - The current external IP and location
  - A **Disabled** option to stop Tailscale
  - Available (online) exit nodes to switch to
  - Offline exit nodes listed below a separator, greyed out
- Exit nodes are read automatically from `tailscale exit-node list` at startup and after each toggle
- Switching nodes runs `tailscale up --exit-node <IP>`, disabling runs `tailscale down`
- No administrator privileges required at runtime (see Prerequisites below)

## Requirements

- GNOME with the [AppIndicator and KStatusNotifierItem Support](https://extensions.gnome.org/extension/615/appindicator-support/) extension enabled
- `tailscale` installed and on your `PATH`
- Rust toolchain (`cargo`)
- System packages: `dbus-devel`, `pkgconf-pkg-config`

On Fedora:

```bash
sudo dnf install dbus-devel pkgconf-pkg-config
```

## Prerequisites

Grant your user permission to control Tailscale without sudo (one-time setup):

```bash
sudo tailscale set --operator=$USER
```

This allows `tailscale up` and `tailscale down` to run as your regular user, which is required for the tray applet to work.

## Building

```bash
cargo build --release
```

The binary will be at `target/release/ts-switcher`.

## Running

```bash
./target/release/ts-switcher
```

To have it start automatically with your GNOME session, create an autostart entry:

```bash
mkdir -p ~/.config/autostart
cat > ~/.config/autostart/ts-switcher.desktop << EOF
[Desktop Entry]
Type=Application
Name=ts-switcher
Exec=/path/to/ts-switcher
X-GNOME-Autostart-enabled=true
EOF
```

Replace `/path/to/ts-switcher` with the full path to the binary.
