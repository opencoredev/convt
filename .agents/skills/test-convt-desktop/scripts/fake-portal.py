#!/usr/bin/env python3
"""A stand-in xdg-desktop-portal for a private D-Bus session bus, so a headless
convt-app can be themed and can open "files" without a real desktop.

    DBUS_SESSION_BUS_ADDRESS=$(cat <dir>/bus) python3 fake-portal.py --private-bus <dir>

<dir>/scheme holds the color scheme the portal reports: 1 for dark, 2 for
light (0 means no preference, which GPUI treats as light). Change the file
and send SIGUSR1 to emit SettingChanged; open windows follow it live.

The FileChooser answers every Open request with the paths listed one per line
in <dir>/pick (cancelling when the file is missing or empty) and appends the
request's title, its `directory` flag and the answer to <dir>/pick.log.

It refuses to start unless it is clearly on a bus the test recipe made, so it
can never take over the user's real portal:

- `--private-bus` is passed;
- DBUS_SESSION_BUS_ADDRESS is set, equals <dir>/bus, and is not the user's
  default bus (`unix:path=$XDG_RUNTIME_DIR/bus` or `/run/user/<uid>/bus`);
- the bus daemon at that address reports the PID in <dir>/buspid, and that
  process is a `dbus-daemon` owned by this user;
- nothing else owns org.freedesktop.portal.Desktop on it. The name is
  requested without queueing, so a real portal is never replaced later.
"""

import os
import signal
import sys

from gi.repository import Gio, GLib

def refuse(why):
    sys.exit(f"fake-portal: refusing to start: {why}")


def check_private_bus(directory):
    address = os.environ.get("DBUS_SESSION_BUS_ADDRESS", "")
    if not address:
        refuse("DBUS_SESSION_BUS_ADDRESS is not set")
    user_buses = {f"/run/user/{os.getuid()}/bus"}
    if os.environ.get("XDG_RUNTIME_DIR"):
        user_buses.add(os.path.join(os.environ["XDG_RUNTIME_DIR"], "bus"))
    path = address.split("unix:path=", 1)[-1].split(",", 1)[0]
    if path in user_buses or address.startswith("unix:path=/run/user/"):
        refuse(f"{address} is the user's session bus")
    try:
        expected = open(os.path.join(directory, "bus")).read().strip()
        pid = int(open(os.path.join(directory, "buspid")).read().strip())
    except (OSError, ValueError):
        refuse(f"{directory}/bus and {directory}/buspid must come from the recipe's dbus-daemon")
    if address != expected:
        refuse(f"the bus address doesn't match {directory}/bus")
    try:
        stat = os.stat(f"/proc/{pid}")
        exe = os.path.basename(os.readlink(f"/proc/{pid}/exe"))
    except OSError:
        refuse(f"the bus daemon {pid} isn't running")
    if stat.st_uid != os.getuid() or exe != "dbus-daemon":
        refuse(f"process {pid} is not this user's dbus-daemon")
    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    reply = bus.call_sync("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
                          "GetConnectionUnixProcessID", GLib.Variant("(s)", ("org.freedesktop.DBus",)),
                          None, Gio.DBusCallFlags.NONE, -1, None)
    if reply.unpack()[0] != pid:
        refuse(f"the bus at {address} is not run by process {pid}")


if len(sys.argv) != 3 or sys.argv[1] != "--private-bus":
    refuse("usage: fake-portal.py --private-bus <dir>")
DIR = sys.argv[2]
check_private_bus(DIR)
SCHEME = os.path.join(DIR, "scheme")
PICK = os.path.join(DIR, "pick")
PATH = "/org/freedesktop/portal/desktop"

XML = """<node>
<interface name='org.freedesktop.portal.Settings'>
  <method name='ReadAll'><arg type='as' direction='in'/><arg type='a{sa{sv}}' direction='out'/></method>
  <method name='Read'><arg type='s' direction='in'/><arg type='s' direction='in'/><arg type='v' direction='out'/></method>
  <method name='ReadOne'><arg type='s' direction='in'/><arg type='s' direction='in'/><arg type='v' direction='out'/></method>
  <signal name='SettingChanged'><arg type='s'/><arg type='s'/><arg type='v'/></signal>
  <property name='version' type='u' access='read'/>
</interface>
<interface name='org.freedesktop.portal.FileChooser'>
  <method name='OpenFile'><arg type='s' direction='in'/><arg type='s' direction='in'/><arg type='a{sv}' direction='in'/><arg type='o' direction='out'/></method>
  <method name='SaveFile'><arg type='s' direction='in'/><arg type='s' direction='in'/><arg type='a{sv}' direction='in'/><arg type='o' direction='out'/></method>
  <property name='version' type='u' access='read'/>
</interface>
</node>"""

conn = None


def scheme():
    try:
        return int(open(SCHEME).read().strip() or 0)
    except (OSError, ValueError):
        return 0


def choose(c, sender, params, inv):
    _parent, title, opts = params.unpack()
    token = opts.get("handle_token", "t")
    request = f"{PATH}/request/{sender[1:].replace('.', '_')}/{token}"
    inv.return_value(GLib.Variant("(o)", (request,)))
    paths = [line.strip() for line in open(PICK)] if os.path.exists(PICK) else []
    uris = [Gio.File.new_for_path(p).get_uri() for p in paths if p]
    with open(PICK + ".log", "a") as log:
        log.write(f"{title} directory={opts.get('directory', False)} {uris}\n")

    def respond():
        code = 0 if uris else 1
        c.emit_signal(sender, request, "org.freedesktop.portal.Request", "Response",
                      GLib.Variant("(ua{sv})", (code, {"uris": GLib.Variant("as", uris)})))
        return False

    GLib.timeout_add(200, respond)


def call(c, sender, _obj, iface, method, params, inv):
    if iface == "org.freedesktop.portal.FileChooser":
        return choose(c, sender, params, inv)
    if method == "ReadAll":
        appearance = {"color-scheme": GLib.Variant("u", scheme())}
        inv.return_value(GLib.Variant("(a{sa{sv}})", ({"org.freedesktop.appearance": appearance},)))
        return
    namespace, key = params.unpack()[:2]
    if (namespace, key) != ("org.freedesktop.appearance", "color-scheme"):
        inv.return_dbus_error("org.freedesktop.portal.Error.NotFound", "not found")
        return
    value = GLib.Variant("u", scheme())
    # Read wraps the value in one more variant than ReadOne.
    inv.return_value(GLib.Variant("(v)", (value if method == "ReadOne" else GLib.Variant("v", value),)))


def prop(_c, _sender, _obj, iface, _name):
    return GLib.Variant("u", 2 if iface.endswith("Settings") else 3)


def acquired(c, _name):
    global conn
    conn = c
    for iface in Gio.DBusNodeInfo.new_for_xml(XML).interfaces:
        c.register_object(PATH, iface, call, prop, None)


def changed():
    value = GLib.Variant("u", scheme())
    conn.emit_signal(None, PATH, "org.freedesktop.portal.Settings", "SettingChanged",
                     GLib.Variant("(ssv)", ("org.freedesktop.appearance", "color-scheme", value)))
    return True


owned = False


def name_acquired(_c, _name):
    global owned
    owned = True


def lost(_c, _name):
    # Exiting from a GLib callback needs os._exit; SystemExit is swallowed.
    why = "lost the portal name" if owned else "org.freedesktop.portal.Desktop is already owned"
    print(f"fake-portal: refusing to run: {why}", file=sys.stderr, flush=True)
    os._exit(1)


GLib.unix_signal_add(GLib.PRIORITY_DEFAULT, signal.SIGUSR1, changed)
Gio.bus_own_name(Gio.BusType.SESSION, "org.freedesktop.portal.Desktop",
                 Gio.BusNameOwnerFlags.DO_NOT_QUEUE, acquired, name_acquired, lost)
GLib.MainLoop().run()
