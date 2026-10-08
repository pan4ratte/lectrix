#!/usr/bin/env python3
"""Scrolls a document with real touchpad input on GNOME (Wayland) and reports how smooth it was.

Scripted scrolling (LECTRIX_PERF=scroll) sets scrollTop from JavaScript, which skips the
webview's own wheel handling. This script starts Lectrix with LECTRIX_PERF=watch and sends
two-finger scroll events through Mutter's remote desktop interface (GNOME shows its
remote-control indicator meanwhile), so they reach the window as a touchpad's would. The
app reports the frames and blank pages it saw while scrolling.

    tests/perf/wheel-linux.py target/release/lectrix file.pdf [--state state.json]

--state runs with a copy of a state file (window place, panes, zoom), never the file itself.
The pointer is moved to the middle of the screen, where the maximized window's pages are.
"""

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
import threading
import time

from gi.repository import Gio, GLib

BUS_NAME = "org.gnome.Mutter.RemoteDesktop"
SESSION = "org.gnome.Mutter.RemoteDesktop.Session"
# Mutter's MetaRemoteDesktopNotifyAxisFlags.
AXIS_FINISH = 1 << 0
AXIS_SOURCE_FINGER = 1 << 2


class Pointer:
    def __init__(self):
        self.bus = Gio.bus_get_sync(Gio.BusType.SESSION)
        reply = self.bus.call_sync(
            BUS_NAME, "/org/gnome/Mutter/RemoteDesktop", BUS_NAME, "CreateSession",
            None, GLib.VariantType("(o)"), 0, -1, None)
        self.path = reply.unpack()[0]
        self.call("Start")

    def call(self, method, args=None):
        self.bus.call_sync(BUS_NAME, self.path, SESSION, method, args, None, 0, -1, None)

    def move(self, dx, dy):
        self.call("NotifyPointerMotionRelative", GLib.Variant("(dd)", (dx, dy)))

    def scroll(self, dy, finish=False):
        flags = AXIS_SOURCE_FINGER | (AXIS_FINISH if finish else 0)
        self.call("NotifyPointerAxis", GLib.Variant("(ddu)", (0.0, dy, flags)))

    def stop(self):
        self.call("Stop")


def swipe(pointer, dy, seconds, rate=120):
    """One two-finger swipe: steady events, then the finish that starts kinetic scrolling."""
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        pointer.scroll(dy)
        time.sleep(1 / rate)
    pointer.scroll(0.0, finish=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("app")
    parser.add_argument("pdf")
    parser.add_argument("--state")
    parser.add_argument("--screen", default="1920x1200", help="logical screen size")
    parser.add_argument("--speed", type=float, default=12.0, help="scroll units per event")
    args = parser.parse_args()

    tmp = tempfile.mkdtemp(prefix="lectrix-wheel-")
    env = dict(os.environ, LECTRIX_PERF="watch")
    if args.state:
        state = os.path.join(tmp, "state.json")
        shutil.copy(args.state, state)
        env["LECTRIX_STATE_FILE"] = state
    else:
        env["LECTRIX_EPHEMERAL"] = "1"
    app = subprocess.Popen([args.app, args.pdf], env=env, stdout=subprocess.PIPE, text=True)
    ready = threading.Event()
    done = threading.Event()
    metrics = []

    def read():
        for line in app.stdout:
            if line.startswith("[lectrix-metric]"):
                metrics.append(line.split(" ", 1)[1].strip())
                if "watch_ready" in line:
                    ready.set()
                if "perf_done" in line:
                    done.set()

    threading.Thread(target=read, daemon=True).start()
    try:
        if not ready.wait(30):
            sys.exit("Lectrix did not start watching")
        time.sleep(0.5)
        pointer = Pointer()
        try:
            width, height = (int(v) for v in args.screen.split("x"))
            pointer.move(-10 * width, -10 * height)
            pointer.move(width / 2, height / 2)
            time.sleep(0.3)
            for direction in (1, -1, 1):
                # A window mapped under a pointer that has not moved since may not have
                # the pointer's focus yet; a small move gives it.
                pointer.move(1, 0)
                pointer.move(-1, 0)
                swipe(pointer, direction * args.speed, 2.5)
                time.sleep(1.2)
        finally:
            pointer.stop()
        done.wait(30)
    finally:
        app.terminate()
        app.wait(10)
        shutil.rmtree(tmp, ignore_errors=True)
    print("\n".join(m for m in metrics if m.startswith(("watch_", "first_page"))))
    if "watch_scroll_events=0.0" in metrics:
        sys.exit("No scroll reached the document: is another window over it?")


if __name__ == "__main__":
    main()
