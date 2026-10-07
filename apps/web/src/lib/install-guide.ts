import type { ArtifactKind } from "./release-manifest";

import type { Os, Slot } from "./platform";

/** The download the page leads with: a published build for `os`, else the first slot. */
export function primarySlot(slots: Slot[], os: Os): Slot | undefined {
  const osSlots = slots.filter((s) => s.os === os);
  return osSlots.find((s) => s.artifact) ?? osSlots[0];
}

export type InstallStep = {
  title: string;
  body: string;
};

/** Three short install steps for the primary download of an OS. */
export function installSteps(os: Os, kind: ArtifactKind): InstallStep[] {
  if (os === "macos") {
    return [
      {
        title: "Open convt.dmg from your Downloads folder",
        body: "The disk image mounts a small window with the convt app inside.",
      },
      {
        title: "Drag the convt icon into your Applications folder",
        body: "That installs the app, the right-click menu and the command line tool.",
      },
      {
        title: "Open convt from Applications",
        body: "macOS may ask you to confirm the first time. After that it is just an app.",
      },
    ];
  }
  if (os === "windows") {
    return [
      {
        title: "Open the installer from your Downloads folder",
        body: "The file is a small installer that only takes a moment.",
      },
      {
        title: "Follow the installer",
        body: "It puts convt on this computer and adds the right-click menu.",
      },
      {
        title: "Open convt from the Start menu",
        body: "Search for convt, or pin it if you want it nearby.",
      },
    ];
  }
  if (kind === "deb") {
    return [
      {
        title: "Open the .deb from your Downloads folder",
        body: "Software, GDebi or your file manager can install it.",
      },
      {
        title: "Install the package",
        body: "Confirm the install. You can also run sudo dpkg -i on the file.",
      },
      {
        title: "Open convt from your app menu",
        body: "The command line tool is on your PATH as convt.",
      },
    ];
  }
  if (kind === "rpm") {
    return [
      {
        title: "Open the .rpm from your Downloads folder",
        body: "GNOME Software, KDE Discover or rpm can install it.",
      },
      {
        title: "Install the package",
        body: "Confirm the install. You can also run sudo rpm -i on the file.",
      },
      {
        title: "Open convt from your app menu",
        body: "The command line tool is on your PATH as convt.",
      },
    ];
  }
  if (kind === "tar.gz") {
    return [
      {
        title: "Unpack the archive",
        body: "Extract it wherever you keep apps. The folder is ready to run.",
      },
      {
        title: "Run the convt binary",
        body: "Inside the folder, launch convt. Add that folder to PATH if you want the CLI everywhere.",
      },
      {
        title: "Optional: install the file-manager menu",
        body: "The bundle includes the Linux menu helper. Run it once if you want right-click conversion.",
      },
    ];
  }
  return [
    {
      title: "Save the AppImage from your Downloads folder",
      body: "Move it somewhere you will find it. It is the whole app in one file.",
    },
    {
      title: "Allow it to run",
      body: "Right-click → Properties → allow executing the file, or chmod +x the AppImage.",
    },
    {
      title: "Open the AppImage",
      body: "Double-click it to start convt. The command line tool is inside the same file.",
    },
  ];
}
