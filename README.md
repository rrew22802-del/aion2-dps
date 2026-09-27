<div align="center">

# DBAion2 DPS

**A free, open-source real-time DPS meter for AION 2, from [dbaion2.ru](https://dbaion2.ru/).**

[![Tauri](https://img.shields.io/badge/Tauri-2.x-24C8DB?logo=tauri&logoColor=white)](https://tauri.app/)
[![React](https://img.shields.io/badge/React-19-61DAFB?logo=react&logoColor=111)](https://react.dev/)
[![Rust](https://img.shields.io/badge/Rust-backend-000000?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-GPL--3.0--only-22C55E)](./LICENSE)

</div>

> [!IMPORTANT]
> **Pre-release.** AION 2 launches globally on **5 October 2026**, Early Access
> from **30 September**. Nothing here has been confirmed against a real global
> session yet — the exact opcodes, the server-id range, and the server-name
> catalogue all get filled in from the first capture.

DBAion2 DPS is a rebranded fork of [Aether](https://github.com/Helveticxa/Aether-Aion2-DPS-meter-Global),
itself a fork of [NOIA2](https://github.com/ZDYoung0519/NOIA2). It stays free, open-source, and
GPL-3.0 — the paid part of dbaion2's AION 2 tools (the farm tracker) is a separate, closed-source
program with no shared code. See [docs/DBAION2.md](./docs/DBAION2.md) for exactly what changed
in this fork and why.

## What it is

DBAion2 DPS reads AION 2 combat data by **passively sniffing network packets**. It
does not read or write game memory, inject code, modify packets, or automate any
part of the game. It is a monitor.

A floating overlay shows live DPS while you fight and steps aside when you
are not. Fights are saved to a searchable history and broken down by skill,
buff uptime, and damage type.

## Features

- **DPS meter** — a rounded glass overlay with class-coloured bars. It appears on
  your first hit, stays for the whole fight, and after five quiet minutes saves
  the fight to History, starts clean, and hides again (configurable)
- **Always on top** — keep your own signed-in Chrome or Edge above the game, with
  opacity and click-through. Works over any borderless game, and tells you when a
  game's exclusive fullscreen blocks every overlay
- **Live chat** — YouTube and Twitch chat over the game, transparent and
  outlined, one pop-up per chat or merged into one
- **Boss fights** — your pace against your personal best while you fight, and a
  summary when the boss dies: DPS, place, crits, top skills, one-click copy
- **Capsule mode** — the meter as one line that opens on hover
- English names for about 8,000 monsters and bosses
- Battle history with per-skill and per-player breakdowns, damage-type split,
  buff timelines, and cast ordering
- Finds the game, your character, and the server on its own — nothing to set up
- Global shortcuts, tray integration, light and dark themes in dbaion2's own
  Nebula palette, English, Korean and Russian
- Runs fully offline — nothing is uploaded, no auto-update phones home

## What it never does

Anti-cheat (BattlEye in PUBG, AION 2's own) watches what other programs do to
the game. This app is built so there is nothing to find:

- It never injects code into a game, loads anything into it, or hooks its
  graphics. That is also why nothing can appear over exclusive fullscreen:
  switch the game to borderless.
- It never opens a game's process, not even to read its name. Which program is
  in front comes from the system's process list, the way Task Manager sees it.
- It never reads or writes game memory, sends input, or automates anything.
- Packets are copied, never touched: Npcap, or WinDivert in sniff and
  receive-only mode. The WinDivert driver is loaded only when Npcap cannot
  capture, and unloaded when this app is done with it.
- Always on top moves only this app's own windows and the browser windows you
  pin. A game's window is only looked at: its size and its style.

None of this is a promise about terms of service. Whether a publisher allows a
third-party tool is its call.

## Install

1. Download the installer from [Releases](../../releases) and run it.
2. Launch DBAion2 DPS **as Administrator** — packet capture requires it.

Windows 10 or 11. No separate driver install: it bundles WinDivert and
checks the capture environment on startup, holding the app closed until it can
actually capture. If nothing is available it offers to install
[Npcap](https://npcap.com/#download) for you.

## Built for the global servers

Every other AION 2 meter targets Korea or Taiwan. This fork exists to work on the
global service from day one — a design choice inherited from Aether, unchanged here.

Aether accepts any structurally valid server id (`1001–1999`, `2001–2999`:
race × 1000 + index), so it parses on every service without being told which
one it is on. There is no region to pick. [FORK.md](./FORK.md) has the full reasoning.

## Protocol tooling

Settings → Aion 2 → Connection → **Advanced** carries packet recording, an
opcode census, and a one-click diagnostics report — all local, nothing leaves
your machine.

## Farm Tracker Pro

The "Farm Tracker Pro" button in Settings → About starts dbaion2's paid farm
tracker if it is already installed, or opens its page on
[dbaion2.ru](https://dbaion2.ru/tracker/) otherwise. It is a separate program
and process; this app never embeds its code.

## Credit

DBAion2 DPS is a rebrand of **[Aether](https://github.com/Helveticxa/Aether-Aion2-DPS-meter-Global)**,
which is itself a fork of **[NOIA2](https://github.com/ZDYoung0519/NOIA2)** by
[zdyoung](https://github.com/ZDYoung0519) — the Rust capture pipeline, the packet
parsers, the overlay, and the game-data catalogues all trace back to that project.

English monster and boss names, the Fighter's skill names, and the server codes
come from **[Kuroukihime/AIon2-Dps-Meter](https://github.com/Kuroukihime/AIon2-Dps-Meter)**
(GPL-3.0), merged by `scripts/build-npc-names.mjs`.

If this app is useful to you, consider supporting [Aether](https://github.com/Helveticxa/Aether-Aion2-DPS-meter-Global)
and [NOIA2](https://ifdian.net/a/zdyoung) directly.

## Build from source

```
pnpm install
pnpm build
```

Then, from an **elevated** terminal:

```
pnpm tauri:dev      # run
pnpm tauri:build    # produce an installer
```

Needs Rust (MSVC toolchain), Node 18+, pnpm, and the MSVC build tools.

See [CLAUDE.md](./CLAUDE.md) for conventions and the gotchas worth knowing before
changing anything, and [docs/DBAION2.md](./docs/DBAION2.md) for what this fork
changed against Aether specifically.

## Disclaimer

This tool reads network traffic on your own machine. It does not inject code,
modify memory, or alter game traffic, and it uploads nothing.

It is still third-party software. Use it at your own discretion and in accordance
with the game's terms of service. Nobody here can promise how NCSoft will treat
any particular tool.

## License

[GPL-3.0-only](./LICENSE), inherited from NOIA2 through Aether. Any distributed
build must ship its source under the same terms.

---

<div align="center">

# DBAion2 DPS (Русский)

**Бесплатный DPS-метр для AION 2 с открытым исходным кодом, от [dbaion2.ru](https://dbaion2.ru/).**

</div>

> [!IMPORTANT]
> **Пререлиз.** Глобальный запуск AION 2 — **5 октября 2026**, ранний доступ — с
> **30 сентября**. Пока ничего не подтверждено на реальной глобальной сессии:
> точные опкоды, диапазон id серверов и справочник имён серверов заполняются
> по мере первых захватов трафика.

DBAion2 DPS — ребрендинг форка **[Aether](https://github.com/Helveticxa/Aether-Aion2-DPS-meter-Global)**,
который сам является форком **[NOIA2](https://github.com/ZDYoung0519/NOIA2)**. Программа остаётся
бесплатной, с открытым кодом, под лицензией GPL-3.0 — платная часть инструментов dbaion2 для
AION 2 (трекер фарма) это отдельная закрытая программа без общего кода. Что именно изменено в
этом форке и почему — в [docs/DBAION2.md](./docs/DBAION2.md).

## Что это

DBAion2 DPS читает боевые данные AION 2 путём **пассивного прослушивания сетевых
пакетов**. Программа не читает и не пишет память игры, не внедряет код, не
изменяет пакеты и не автоматизирует ничего в игре — это монитор.

Плавающий оверлей показывает DPS в реальном времени во время боя и уходит с
глаз, когда боя нет. Бои сохраняются в историю с поиском и разбором по
навыкам, времени баффов и типу урона.

## Возможности

- **DPS-метр** — скруглённая полупрозрачная панель с полосками в цвете класса.
  Появляется на первый удар, остаётся на весь бой, после пяти минут тишины
  сохраняет бой в историю, сбрасывается и снова прячется (настраивается)
- **Поверх всех окон** — держите своё окно Chrome или Edge поверх игры, с
  прозрачностью и кликами насквозь. Работает над любой игрой в оконном режиме
  без рамки, и сообщает, если полноэкранный режим игры блокирует все оверлеи
- **Живой чат** — чат YouTube и Twitch поверх игры, прозрачный, с обводкой,
  отдельным окном на каждый чат или объединённый в одно
- **Боссы** — ваш темп относительно личного рекорда прямо во время боя и
  итоговая сводка при смерти босса: DPS, место, криты, топ-навыки, копирование
  в один клик
- **Капсульный режим** — метр в виде одной строки, разворачивающейся при наведении
- Английские названия примерно 8000 монстров и боссов
- История боёв с разбором по навыкам и игрокам, по типу урона, таймлайном баффов
- Сама находит игру, вашего персонажа и сервер — ничего не нужно настраивать
- Глобальные горячие клавиши, интеграция с треем, светлая и тёмная темы в
  собственной палитре Nebula от dbaion2, английский, корейский и русский языки
- Работает полностью офлайн — ничего не отправляется, автообновление отключено

## Чего программа никогда не делает

Античит (BattlEye в PUBG, собственный у AION 2) следит за тем, что другие
программы делают с игрой. Эта программа устроена так, что искать нечего:

- Никогда не внедряет код в игру, ничего в неё не загружает и не перехватывает
  графику. Поэтому ничего не может появиться поверх эксклюзивного полноэкранного
  режима — переключите игру в оконный режим без рамки.
- Никогда не открывает процесс игры, даже чтобы прочитать его имя. Какая
  программа активна — узнаётся из системного списка процессов, как это видит
  Диспетчер задач.
- Никогда не читает и не пишет память игры, не отправляет ввод, ничего не
  автоматизирует.
- Пакеты копируются, но не изменяются: Npcap либо WinDivert в режиме только
  прослушивания/приёма. Драйвер WinDivert загружается, только если Npcap не
  может захватывать трафик, и выгружается, когда программа закрывается.
- «Поверх всех окон» двигает только собственные окна программы и закреплённые
  вами окна браузера. Окно игры только просматривается: его размер и стиль.

Это не гарантия по условиям использования игры. Разрешает ли издатель
сторонние инструменты — решает он сам.

## Установка

1. Скачайте установщик со страницы [Releases](../../releases) и запустите его.
2. Запускайте DBAion2 DPS **от имени администратора** — захват пакетов этого требует.

Windows 10 или 11. Отдельно ставить драйвер не нужно: WinDivert уже внутри, и
при запуске программа проверяет окружение захвата, не открываясь, пока захват
невозможен. Если ничего не доступно — предложит установить
[Npcap](https://npcap.com/#download).

## Farm Tracker Pro

Кнопка «Farm Tracker Pro» в Настройках → О программе запускает платный трекер
фарма dbaion2, если он уже установлен, иначе открывает его страницу на
[dbaion2.ru](https://dbaion2.ru/tracker/). Это отдельная программа и процесс —
код этой программы нигде в неё не встраивается.

## Благодарности

DBAion2 DPS — ребрендинг **[Aether](https://github.com/Helveticxa/Aether-Aion2-DPS-meter-Global)**,
который сам является форком **[NOIA2](https://github.com/ZDYoung0519/NOIA2)** авторства
[zdyoung](https://github.com/ZDYoung0519) — оттуда происходят конвейер захвата на Rust, разбор
пакетов, оверлей и справочники игровых данных.

Английские названия монстров и боссов, названия навыков Бойца и коды серверов —
из **[Kuroukihime/AIon2-Dps-Meter](https://github.com/Kuroukihime/AIon2-Dps-Meter)**
(GPL-3.0), объединены скриптом `scripts/build-npc-names.mjs`.

Если программа вам полезна — поддержите напрямую
[Aether](https://github.com/Helveticxa/Aether-Aion2-DPS-meter-Global) и
[NOIA2](https://ifdian.net/a/zdyoung).

## Сборка из исходников

```
pnpm install
pnpm build
```

Затем, из терминала **с правами администратора**:

```
pnpm tauri:dev      # запуск
pnpm tauri:build    # сборка установщика
```

Нужны Rust (тулчейн MSVC), Node 18+, pnpm и средства сборки MSVC.

Смотрите [CLAUDE.md](./CLAUDE.md) об соглашениях и подводных камнях, и
[docs/DBAION2.md](./docs/DBAION2.md) о том, что именно этот форк поменял
относительно Aether.

## Отказ от ответственности

Эта программа читает сетевой трафик на вашем собственном компьютере. Она не
внедряет код, не изменяет память и не подменяет игровой трафик, и ничего
никуда не отправляет.

Это всё равно стороннее программное обеспечение. Используйте его на свой
страх и риск и в соответствии с условиями использования игры. Никто здесь не
может обещать, как NCSoft отнесётся к тому или иному инструменту.

## Лицензия

[GPL-3.0-only](./LICENSE), унаследована от NOIA2 через Aether. Любая
распространяемая сборка должна поставляться с исходным кодом на тех же условиях.
