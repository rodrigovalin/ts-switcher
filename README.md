# ts-switcher

A GNOME system tray applet to toggle a Tailscale exit node on and off.

## What it does

- Shows a grey circle icon in the GNOME top bar
  - **Hollow circle** — exit node is disabled (Tailscale is stopped)
  - **Filled circle** — exit node is enabled
- Clicking the icon opens a menu to switch between **Disabled** and **Enabled**
- Enabling runs `tailscale up --exit-node <IP>`, disabling runs `tailscale down`
- Both commands require administrator privileges — a GNOME authentication dialog will appear automatically

## Requirements

- GNOME with the [AppIndicator and KStatusNotifierItem Support](https://extensions.gnome.org/extension/615/appindicator-support/) extension enabled
- `tailscale` installed and on your `PATH`
- `pkexec` (ships with polkit, present on any standard Fedora/GNOME install)
- Rust toolchain (`cargo`)
- System packages: `dbus-devel`, `pkgconf-pkg-config`

On Fedora:

```bash
sudo dnf install dbus-devel pkgconf-pkg-config
```

## Configuration

Create the config file with your exit node IP address:

```bash
mkdir -p ~/.config/ts-switcher
echo "100.x.x.x" > ~/.config/ts-switcher/exit_node.env
```

The file must contain a single line with the IP address of your Tailscale exit node. The app will not start if this file is missing.

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
