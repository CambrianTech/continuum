#!/usr/bin/env python3
"""continuum-tray — Linux system tray for Continuum.

Reads `continuum tray-data` JSON. Renders GTK AppIndicator or fallback.
Requires: python3-gi (GTK), optionally libappindicator3 for Ubuntu/GNOME.

Install deps: sudo apt install python3-gi gir1.2-appindicator3-0.1
Run: python3 continuum-tray.py &
"""

import json
import os
import subprocess
import sys
import threading
import time

# Try GTK AppIndicator (Ubuntu/GNOME), fall back to basic Gtk.StatusIcon
try:
    import gi
    gi.require_version('Gtk', '3.0')
    from gi.repository import Gtk, GLib
    try:
        gi.require_version('AppIndicator3', '0.1')
        from gi.repository import AppIndicator3
        HAS_APPINDICATOR = True
    except (ValueError, ImportError):
        HAS_APPINDICATOR = False
    HAS_GTK = True
except ImportError:
    HAS_GTK = False
    print("GTK not available. Install: sudo apt install python3-gi gir1.2-appindicator3-0.1", file=sys.stderr)
    sys.exit(1)

CONTINUUM_CLI = os.path.expanduser("~/.local/bin/continuum")
if not os.path.exists(CONTINUUM_CLI):
    CONTINUUM_CLI = "continuum"  # hope it's in PATH

def get_tray_data():
    try:
        result = subprocess.run(
            [CONTINUUM_CLI, "tray-data"],
            capture_output=True, text=True, timeout=10
        )
        return json.loads(result.stdout)
    except Exception:
        return {
            "status": "red", "statusText": "CLI error",
            "docker": False, "nodes": [], "actions": []
        }

class ContinuumTray:
    def __init__(self):
        self.data = get_tray_data()

        if HAS_APPINDICATOR:
            self.indicator = AppIndicator3.Indicator.new(
                "continuum", "network-idle",
                AppIndicator3.IndicatorCategory.APPLICATION_STATUS
            )
            self.indicator.set_status(AppIndicator3.IndicatorStatus.ACTIVE)
        else:
            self.indicator = None

        self.build_menu()
        # Refresh every 30 seconds
        GLib.timeout_add_seconds(30, self.refresh)

    def status_icon_name(self):
        return {
            "green": "network-transmit-receive",
            "yellow": "network-idle",
            "red": "network-error",
        }.get(self.data.get("status", "red"), "network-offline")

    def build_menu(self):
        menu = Gtk.Menu()

        # Status header
        header = Gtk.MenuItem(label=self.data.get("statusText", "Unknown"))
        header.set_sensitive(False)
        menu.append(header)
        menu.append(Gtk.SeparatorMenuItem())

        # Actions
        for action in self.data.get("actions", []):
            item = Gtk.MenuItem(label=action["label"])
            cmd = action["command"]
            item.connect("activate", lambda w, c=cmd: self.run_command(c))
            menu.append(item)

        # Nodes
        nodes = self.data.get("nodes", [])
        if nodes:
            menu.append(Gtk.SeparatorMenuItem())
            for node in nodes:
                icon = "●" if node.get("online") else "○"
                item = Gtk.MenuItem(label=f"{icon} {node['name']}")
                url = node.get("url")
                if url:
                    item.connect("activate", lambda w, u=url: self.open_url(u))
                else:
                    item.set_sensitive(False)
                menu.append(item)

        menu.append(Gtk.SeparatorMenuItem())
        quit_item = Gtk.MenuItem(label="Quit Continuum Tray")
        quit_item.connect("activate", lambda w: Gtk.main_quit())
        menu.append(quit_item)

        menu.show_all()

        if self.indicator:
            self.indicator.set_icon(self.status_icon_name())
            self.indicator.set_menu(menu)

    def refresh(self):
        self.data = get_tray_data()
        self.build_menu()
        return True  # keep timer running

    def run_command(self, cmd):
        threading.Thread(
            target=lambda: subprocess.run(cmd, shell=True),
            daemon=True
        ).start()

    def open_url(self, url):
        subprocess.Popen(["xdg-open", url])

if __name__ == "__main__":
    tray = ContinuumTray()
    Gtk.main()
