import { formatBytes, type ArtifactKind } from "./release-manifest";

import { kindLabels, type Os, type Slot } from "./platform";

/** The download the page leads with: a published build for `os`, else the first slot. */
export function primarySlot(slots: Slot[], os: Os): Slot | undefined {
  const osSlots = slots.filter((s) => s.os === os);
  return osSlots.find((s) => s.artifact) ?? osSlots[0];
}

/** The slot the visitor selected, or the primary download when `kind` is missing or not for `os`. */
export function selectedSlot(slots: Slot[], os: Os, kind?: ArtifactKind): Slot | undefined {
  if (kind) {
    const match = slots.find((s) => s.os === os && s.kind === kind);
    if (match) return match;
  }
  return primarySlot(slots, os);
}

/** Human label for the slot's processor, e.g. Apple silicon for macos-arm64. */
export function processorLabel(slot: Pick<Slot, "os" | "arch">): string {
  if (slot.os === "macos" && slot.arch === "arm64") return "Apple silicon";
  if (slot.os === "macos" && slot.arch === "x86_64") return "Intel";
  if (slot.arch === "x86_64") return "64-bit";
  return slot.arch;
}

/** Kind, processor and size (or the unpublished note) shown under the download button. */
export function slotCaption(slot: Slot): string {
  const processor = processorLabel(slot);
  const parts = [kindLabels[slot.kind].title, processor];
  if (slot.artifact) {
    parts.push(formatBytes(slot.artifact.size));
  } else if (kindLabels[slot.kind].note !== processor) {
    parts.push(kindLabels[slot.kind].note);
  }
  return parts.join(" · ");
}

export type InstallStep = {
  title: string;
  body: string;
};

/** Three short install steps for the selected download of an OS. */
export function installSteps(os: Os, kind: ArtifactKind): InstallStep[] {
  if (kind === "zip") return zipSteps(os);
  if (os === "macos") return macDmgSteps();
  if (os === "windows") return windowsInstallerSteps();
  if (kind === "deb") return debSteps();
  if (kind === "rpm") return rpmSteps();
  if (kind === "tar.gz") return tarSteps();
  return appImageSteps();
}

function zipSteps(os: Os): InstallStep[] {
  if (os === "macos") {
    return [
      {
        title: "Open the zip from your Downloads folder",
        body: "Finder unpacks it into a folder with convt.app inside.",
      },
      {
        title: "Drag the convt icon into your Applications folder",
        body: "That puts the app on this Mac. The zip has no installer.",
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
        title: "Open the zip from your Downloads folder",
        body: "Extract the folder wherever you keep apps.",
      },
      {
        title: "Open the extracted folder",
        body: "It holds the app files. There is no installer and no Start menu shortcut.",
      },
      {
        title: "Run convt-app.exe",
        body: "Double-click it to start convt. The command line tool in the same folder is convt.exe.",
      },
    ];
  }
  return [
    {
      title: "Unpack the archive",
      body: "Extract it wherever you keep apps. The folder is ready to run.",
    },
    {
      title: "Run ./convt-app",
      body: "Inside the folder, launch ./convt-app to open the app. The command line tool is ./convt.",
    },
    {
      title: "Optional: add the folder to PATH",
      body: "Then you can run convt from any terminal.",
    },
  ];
}

function macDmgSteps(): InstallStep[] {
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

function windowsInstallerSteps(): InstallStep[] {
  return [
    {
      title: "Open the installer from your Downloads folder",
      body: "The file is a small installer that only takes a moment.",
    },
    {
      title: "Follow the installer",
      body: "It puts convt on this computer and adds a Start menu shortcut.",
    },
    {
      title: "Open convt from the Start menu",
      body: "Search for convt, or pin it if you want it nearby.",
    },
  ];
}

function debSteps(): InstallStep[] {
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

function rpmSteps(): InstallStep[] {
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

function tarSteps(): InstallStep[] {
  return [
    {
      title: "Unpack the archive",
      body: "Extract it wherever you keep apps. The folder is ready to run.",
    },
    {
      title: "Run ./convt-app",
      body: "Inside the folder, launch ./convt-app to open the app. The command line tool is ./convt.",
    },
    {
      title: "Optional: add the folder to PATH",
      body: "Then you can run convt from any terminal. File-manager menus ship with the .deb and .rpm packages.",
    },
  ];
}

function appImageSteps(): InstallStep[] {
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
