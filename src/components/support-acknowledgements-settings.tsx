import { openUrl } from "@tauri-apps/plugin-opener";
import { ExternalLink } from "lucide-react";

import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { SettingsGroup, SettingsSectionHeader } from "@/components/settings-layout";

type CreditItem = {
  name: string;
  description: string;
  href: string;
  avatarSrc?: string;
  avatarFallback: string;
  badge: string;
};

/**
 * DBAion2 DPS is a fork of a fork, and this page says so plainly.
 *
 * Upstream's own page collected donations and community group numbers; those
 * belong to that project and its author, not to this fork, so they are not
 * reproduced here. The credit itself is.
 */
const UPSTREAM: CreditItem = {
  name: "ZDYoung0519/NOIA2",
  description:
    "NOIA2 by zdyoung, forked by Aether: the capture pipeline, the packet parsers, the overlay, and the game-data catalogues all come from that project.",
  href: "https://github.com/ZDYoung0519/NOIA2",
  avatarSrc: "https://avatars.githubusercontent.com/u/60741049?s=80&v=4",
  avatarFallback: "NO",
  badge: "Upstream",
};

const TECHNICAL_CREDITS: CreditItem[] = [
  {
    name: "TK-open-public/Aion2-Dps-Meter",
    description: "Reference for AION2 packet parsing, and the source of the Korean server block.",
    href: "https://github.com/TK-open-public/Aion2-Dps-Meter",
    avatarSrc: "https://avatars.githubusercontent.com/u/253818446?s=80&v=4",
    avatarFallback: "TK",
    badge: "Reference",
  },
  {
    name: "taengu/Aion2-Dps-Meter",
    description: "Another open implementation of an AION2 DPS meter.",
    href: "https://github.com/taengu/Aion2-Dps-Meter",
    avatarSrc: "https://avatars.githubusercontent.com/u/7606218?s=80&v=4",
    avatarFallback: "TG",
    badge: "Reference",
  },
  {
    name: "p62003/aletheia_AION2_DPS_Meter",
    description: "Reference for combat-data presentation and analysis.",
    href: "https://github.com/p62003/aletheia_AION2_DPS_Meter",
    avatarSrc: "https://avatars.githubusercontent.com/u/125135560?s=80&v=4",
    avatarFallback: "P6",
    badge: "Reference",
  },
];

/** Work that is not code: what the app shows, rather than how it runs. */
const ARTWORK_CREDITS: CreditItem[] = [
  {
    name: "Kuroukihime/AIon2-Dps-Meter",
    description:
      "The English names of monsters, bosses, and the Fighter's skills, and the server codes. GPL-3.0, like this project.",
    href: "https://github.com/Kuroukihime/AIon2-Dps-Meter",
    avatarFallback: "KU",
    badge: "Game data",
  },
];

/**
 * This build's own upstream: DBAion2 DPS is a rebrand of Aether, itself a
 * fork of NOIA2 (see UPSTREAM above). Aether's animated "Dune" background is
 * not reproduced here — it was not GPL-licensed — so there is nothing to
 * credit for it.
 */
const FORK_CREDIT: CreditItem = {
  name: "Helveticxa/Aether-Aion2-DPS-meter-Global",
  description:
    "DBAion2 DPS is a rebrand of Aether: the capture pipeline, packet parsers and overlay come from that project's own fork of NOIA2.",
  href: "https://github.com/Helveticxa/Aether-Aion2-DPS-meter-Global",
  avatarFallback: "AE",
  badge: "This build's base",
};

function openExternalLink(href: string) {
  void openUrl(href);
}

function CreditRow({ item }: { item: CreditItem }) {
  return (
    <div className="flex items-center justify-between gap-4 px-5 py-4">
      <div className="flex min-w-0 items-center gap-3">
        <Avatar size="lg">
          {item.avatarSrc ? <AvatarImage src={item.avatarSrc} alt={item.name} /> : null}
          <AvatarFallback>{item.avatarFallback}</AvatarFallback>
        </Avatar>
        <div className="flex min-w-0 flex-col gap-1">
          <div className="flex min-w-0 items-center gap-2">
            <span className="truncate text-sm font-medium">{item.name}</span>
            <Badge variant="secondary">{item.badge}</Badge>
          </div>
          <p className="text-muted-foreground text-xs leading-5">{item.description}</p>
        </div>
      </div>
      <Button variant="outline" size="sm" onClick={() => openExternalLink(item.href)}>
        <ExternalLink data-icon="inline-start" />
        Open
      </Button>
    </div>
  );
}

export function SupportAcknowledgementsSettings() {
  return (
    <div className="flex flex-col gap-8">
      <SettingsSectionHeader
        title="Credits"
        description="DBAion2 DPS stands on other people's work. This page records whose."
      />

      <SettingsGroup title="Built on">
        <CreditRow item={FORK_CREDIT} />
        <CreditRow item={UPSTREAM} />
        <div className="text-muted-foreground px-5 pb-5 text-xs leading-5">
          All three projects are licensed GPL-3.0-only. If this app is useful to you, consider
          supporting Aether and NOIA2 directly through the links on their own repositories.
        </div>
      </SettingsGroup>

      <SettingsGroup title="References">
        {TECHNICAL_CREDITS.map((item) => (
          <CreditRow key={item.href} item={item} />
        ))}
      </SettingsGroup>

      <SettingsGroup title="Artwork and data">
        {ARTWORK_CREDITS.map((item) => (
          <CreditRow key={item.href} item={item} />
        ))}
      </SettingsGroup>
    </div>
  );
}
